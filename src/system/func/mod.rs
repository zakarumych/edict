mod action;
mod res;
mod state;
mod view;
mod world;

use core::{
    any::{type_name, TypeId},
    marker::PhantomData,
    ptr::NonNull,
};

use crate::{archetype::Archetype, component::ComponentInfo, world::World};

use super::{Access, ActionBufferQueue, IntoSystem, System};

pub use self::{
    action::ActionEncoderState,
    res::{ResLocal, ResMutLocal, ResMutNoSendState, ResMutState, ResNoSyncState, ResState},
    state::{State, StateState},
    view::QueryArg,
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

    /// Checks if this argument will visit specified archetype.
    /// Called only for scheduling purposes.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns true if components accessed by the argument are borrowed at runtime,
    /// allowing other args that conflict with it run if they too
    /// borrow components at runtime.
    #[must_use]
    fn borrows_components_at_runtime(&self) -> bool;

    /// Returns access type to the specified component type this argument may perform.
    /// Called only for scheduling purposes.
    #[must_use]
    fn component_access(&self, comp: &ComponentInfo) -> Option<Access>;

    /// Returns access type to the specified resource type this argument may perform.
    /// Called only for scheduling purposes.
    #[must_use]
    fn resource_type_access(&self, ty: TypeId) -> Option<Access>;

    /// Extracts argument from the world.
    /// This method is called with synchronization guarantees provided
    /// according to requirements returned by [`FnArgState::is_local`], [`FnArgState::world_access`],
    /// [`FnArgState::visit_archetype`], [`FnArgState::component_access`] and [`FnArgState::resource_type_access`].
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
        queue: &mut dyn ActionBufferQueue,
    ) -> Self::Arg<'a>;

    /// Flushes the argument state.
    /// This method is called after system execution, when `Arg` is already dropped.
    ///
    /// # Safety
    ///
    /// `world` may be dereferenced mutably only if [`FnArgState::world_access`] returns [`Access::Write`]
    /// and [`FnArgState::is_local`] returns `true`.
    /// Otherwise `world` may be dereferenced immutably only if [`FnArgState::world_access`] returns [`Access::Read`].
    /// Otherwise `world` must not be dereferenced.
    #[inline]
    unsafe fn flush_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionBufferQueue) {
        let _ = world;
        let _ = queue;
    }
}

/// Types that can be used as arguments for function-systems.
#[diagnostic::on_unimplemented(label = "`{Self}` cannot be used as function-system argument")]
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
            #[inline]
            fn is_local(&self) -> bool {
                let ($($a,)*) = &self.args;
                false $( || $a.is_local() )*
            }

            #[inline]
            fn world_access(&self) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut result = None;
                $(
                    result = match (result, $a.world_access()) {
                        (None, one) | (one, None) => one,
                        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
                        _ => {
                            panic!("Mutable `World` aliasing in system `{}`", type_name::<Self>());
                        }
                    };
                )*
                result
            }

            #[inline]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)*) = &self.args;
                false $( || $a.visit_archetype(archetype) )*
            }

            #[inline]
            fn component_access(&self, archetype: &Archetype, comp: &ComponentInfo) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut result = None;
                let mut runtime_borrow = true;
                $(
                    if $a.visit_archetype(archetype) {
                        if let Some(access) = $a.component_access(comp) {
                            runtime_borrow &= $a.borrows_components_at_runtime();
                            result = match (result, access) {
                                (None, one) => Some(one),
                                (Some(Access::Read), Access::Read) => Some(Access::Read),
                                (Some(Access::Write), _) | (Some(_), Access::Write) => {
                                    if runtime_borrow {
                                        // All args that access this component use runtime borrow.
                                        // Conflict will be resolved at runtime.
                                        Some(Access::Write)
                                    } else {
                                        panic!("Conflicting args in system `{}`.\nA component is aliased mutably.\nIf arguments require mutable aliasing, all arguments that access a type must use runtime borrow check.\nFor example `View` type does not use runtime borrow check and should be replaced with `ViewCell`.", type_name::<Func>());
                                    }
                                }
                            };
                        }
                    }
                )*
                result
            }

            #[inline]
            fn resource_type_access(&self, ty: TypeId) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut result = None;
                $(
                    if let Some(access) = $a.resource_type_access(ty) {
                        result = match (result, access) {
                            (None, one) => Some(one),
                            (Some(Access::Read), Access::Read) => Some(Access::Read),
                            (Some(Access::Write), _) | (Some(_), Access::Write) => {
                                panic!("Conflicting args in system `{}`.
                                    A resource is aliased mutably.",
                                    type_name::<$a>());
                            }
                        };
                    }
                )*
                result
            }

            #[inline]
            unsafe fn run_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionBufferQueue) {
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

            #[inline]
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
    #[inline]
    fn from_world(_: &World) -> Self {
        T::default()
    }
}
