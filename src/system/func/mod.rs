mod action;
// mod query;
mod res;
mod state;
mod world;

use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use super::{ActionQueue, IntoSystem, System};
use crate::{
    archetype::Archetype,
    query::{merge_access, Access},
    world::World,
};

pub use self::{
    action::ActionEncoderCache,
    // query::{QueryArg, QueryArgCache, QueryArgGet, QueryRefCache},
    res::{
        Res, ResCache, ResMut, ResMutCache, ResMutNoSend, ResMutNoSendCache, ResNoSync,
        ResNoSyncCache,
    },
    state::{State, StateCache},
    world::{WorldReadCache, WorldWriteCache},
};

/// Marker for [`IntoSystem`] for functions.
pub struct IsFunctionSystem<Args> {
    marker: PhantomData<fn(Args)>,
}

/// Cache for an argument that is stored between calls to function-system.
pub unsafe trait FnArgGet<'a> {
    /// Argument supplied to function-system.
    type Arg: FnArg<Cache = Self> + 'a;

    /// Extracts argument from the world.
    unsafe fn get_unchecked(
        &'a mut self,
        world: NonNull<World>,
        queue: &mut dyn ActionQueue,
    ) -> Self::Arg;

    /// Flushes cache to the world.
    /// This method provides an opportunity for argument cache to do a cleanup of flushing.
    ///
    /// For instance `ActionEncoderCache` - a cache type for `ActionEncoder` argument - flushes `ActionEncoder` to `ActionQueue`.
    #[inline(always)]
    unsafe fn flush_unchecked(&'a mut self, _world: NonNull<World>, _queue: &mut dyn ActionQueue) {}
}

/// Cache for an argument that is stored between calls to function-system.
///
/// # Safety
///
/// If [`FnArgCache::is_local`] returns false [`FnArgGet::get_unchecked`] must be safe to call from any thread.
/// Otherwise [`FnArgGet::get_unchecked`] must be safe to call from local thread.
pub trait FnArgCache: for<'a> FnArgGet<'a> + Send + 'static {
    /// Constructs new cache instance.
    #[must_use]
    fn new() -> Self;

    /// Returns `true` for local arguments that can be used only for local function-systems.
    #[must_use]
    fn is_local(&self) -> bool;

    /// Returns access type performed on the entire [`World`].
    /// Most arguments will return some [`Access::Read`], and few will return none.
    #[must_use]
    fn world_access(&self) -> Option<Access>;

    /// Checks if this argument will skip specified archetype.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns access type to the specified component type this argument may perform.
    #[must_use]
    fn access_component(&self, id: TypeId) -> Option<Access>;

    /// Returns access type to the specified resource type this argument may perform.
    #[must_use]
    fn access_resource(&self, id: TypeId) -> Option<Access>;
}

/// Types that can be used as an argument for function-systems.
pub trait FnArg {
    /// State for an argument that is stored between calls to function-system.
    type Cache: FnArgCache;
}

/// Wrapper for function-like values and implements [`System`].
pub struct FunctionSystem<F, Args> {
    f: F,
    args: Args,
}

macro_rules! impl_func {
    ($($a:ident)*) => {
        #[allow(unused_variables, unused_mut, non_snake_case)]
        unsafe impl<Func $(,$a)*> System for FunctionSystem<Func, ($($a,)*)>
        where
            $($a: FnArgCache,)*
            Func: for<'a> FnMut($(
                <$a as FnArgGet<'a>>::Arg,
            )*),
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
            Func: FnMut($($a,)*) + Send + 'static,
            Func: for<'a> FnMut($(
                <$a::Cache as FnArgGet<'a>>::Arg,
            )*),
        {
            type System = FunctionSystem<Self, ($($a::Cache,)*)>;

            #[inline(always)]
            fn into_system(self) -> Self::System {
                FunctionSystem {
                    f: self,
                    args: ($($a::Cache::new(),)*),
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
