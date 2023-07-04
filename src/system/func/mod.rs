mod action;
mod res;
mod state;
mod view;
mod world;

use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use super::{ActionQueue, IntoSystem, System};
use crate::{
    archetype::Archetype,
    query::{merge_access, Access},
    world::World,
};

pub use self::{
    action::ActionEncoderState,
    res::{
        Res, ResMut, ResMutNoSend, ResMutNoSendState, ResMutState, ResNoSync, ResNoSyncState,
        ResState,
    },
    state::{State, StateState},
    view::QueryArg,
    world::{WorldReadState, WorldWriteState},
};

/// Marker for [`IntoSystem`] for functions.
pub struct IsFunctionSystem<Args> {
    marker: PhantomData<fn(Args)>,
}

/// State for an argument that is stored between calls to function-system.
///
/// # Safety
///
/// If [`FnArgState::is_local`] returns false [`FnArgState::get_unchecked`] must be safe to call from any thread.
/// Otherwise [`FnArgState::get_unchecked`] must be safe to call from local thread.
pub unsafe trait FnArgState: Send + 'static {
    /// Corresponding argument type of the function-system.
    type Arg<'a>: FnArg<State = Self> + 'a;

    /// Constructs the state instance.
    #[must_use]
    fn new() -> Self;

    /// Returns `true` for local arguments that can be used only for local function-systems.
    ///
    /// If this function returns `false` - executor may call `get_unchecked` from any thread.
    /// Otherwise `get_unchecked` executor must call `get_unchecked` from the thread,
    /// where executor is running.
    #[must_use]
    fn is_local(&self) -> bool;

    /// Returns access type performed on the entire [`World`].
    ///
    /// Return [`Access::Write`] if argument allows world mutation -
    /// `&mut World` or similar.
    /// Note that `&mut World`-like arguments also requires `is_local` to return `true`.
    /// Most arguments will return some [`Access::Read`].
    /// If argument doesn't access the world at all - return `None`.
    #[must_use]
    fn world_access(&self) -> Option<Access>;

    /// Checks if this argument will skip specified archetype.
    /// Called only for scheduling purposes.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns access type to the specified component type this argument may perform.
    /// Called only for scheduling purposes.
    #[must_use]
    fn access_component(&self, id: TypeId) -> Option<Access>;

    /// Returns access type to the specified resource type this argument may perform.
    /// Called only for scheduling purposes.
    #[must_use]
    fn access_resource(&self, id: TypeId) -> Option<Access>;

    /// Extracts argument from the world.
    /// This method is called with synchronization guarantees provided
    /// according to requirements returned by [`FnArgState::is_local`], [`FnArgState::world_access`],
    /// [`FnArgState::visit_archetype`], [`FnArgState::access_component`] and [`FnArgState::access_resource`].
    ///
    /// # Safety
    ///
    /// `world` may be dereferenced mutably only if [`FnArgState::world_access`] returns [`Access::Write`]
    /// and [`FnArgState::is_local`] returns `true`.
    /// Otherwise `world` may be dereferenced immutably only if [`FnArgState::world_access`] returns [`Access::Read`].
    /// Otherwise `world` must not be dereferenced.
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        queue: &mut dyn ActionQueue,
    ) -> Self::Arg<'a>;

    /// Flushes the argument state.
    /// This method is called after system execution, when `Arg` is already dropped.
    #[inline(always)]
    unsafe fn flush_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionQueue) {
        drop(world);
        drop(queue);
    }
}

/// Types that can be used as arguments for function-systems.
pub trait FnArg {
    /// State for an argument that is stored between calls to function-system.
    type State: FnArgState;
}

/// Wrapper for function-like values and implements [`System`].
pub struct FunctionSystem<F, ArgStates> {
    f: F,
    args: ArgStates,
}

macro_rules! impl_func {
    ($($a:ident)*) => {
        #[allow(unused_variables, unused_mut, non_snake_case)]
        unsafe impl<Func $(,$a)*> System for FunctionSystem<Func, ($($a,)*)>
        where
            $($a: FnArgState,)*
            Func: for<'a> FnMut($($a::Arg<'a>,)*),
        {
            #[inline(always)]
            fn is_local(&self) -> bool {
                let ($($a,)*) = &self.args;
                false $( || $a.is_local() )*
            }

            #[inline(always)]
            fn world_access(&self) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut result = None;
                $(result = merge_access::<Func>(result, $a.world_access());)*
                result
            }

            #[inline(always)]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)*) = &self.args;
                false $( || $a.visit_archetype(archetype) )*
            }

            #[inline(always)]
            fn access_component(&self, id: TypeId) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut result = None;
                $(result = merge_access::<Func>(result, $a.world_access());)*
                result
            }

            #[inline(always)]
            fn access_resource(&self, id: TypeId) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut result = None;
                $(result = merge_access::<Func>(result, $a.access_resource(id));)*
                result
            }

            #[inline(always)]
            unsafe fn run_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionQueue) {
                let ($($a,)*) = &mut self.args;

                {
                    $(
                        let $a = unsafe { $a.get_unchecked(world, queue) };
                    )*

                    (self.f)($($a,)*);
                }

                $(
                    unsafe { $a.flush_unchecked(world, queue) };
                )*
            }
        }

        impl<Func $(, $a)*> IntoSystem<IsFunctionSystem<($($a,)*)>> for Func
        where
            $($a: FnArg,)*
            Func: FnMut($($a),*) + Send + 'static,
            Func: for<'a> FnMut($(<$a::State as FnArgState>::Arg<'a>),*),
        {
            type System = FunctionSystem<Self, ($($a::State,)*)>;

            #[inline(always)]
            fn into_system(self) -> Self::System {
                FunctionSystem {
                    f: self,
                    args: ($($a::State::new(),)*),
                }
            }
        }
    }
}

for_tuple!(impl_func);

/// Trait for values that can be created from [`World`] reference.
pub trait FromWorld {
    /// Returns new value created from [`World`] reference.
    #[must_use]
    fn from_world(world: &World) -> Self;
}

impl<T> FromWorld for T
where
    T: Default,
{
    #[inline(always)]
    fn from_world(_: &World) -> Self {
        T::default()
    }
}
