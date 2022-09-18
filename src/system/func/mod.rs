mod action;
mod query;
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
    query::{QueryArg, QueryArgCache, QueryArgGet, QueryRefCache},
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
    #[inline]
    unsafe fn flush_unchecked(&'a mut self, _world: NonNull<World>, _queue: &mut dyn ActionQueue) {}
}

/// Cache for an argument that is stored between calls to function-system.
///
/// # Safety
///
/// If [`FnArgCache::is_local`] returns false [`FnArgGet::get_unchecked`] must be safe to call from any thread.
/// Otherwise [`FnArgGet::get_unchecked`] must be safe to call from local thread.
pub trait FnArgCache: for<'a> FnArgGet<'a> + Default + Send + 'static {
    /// Returns `true` for local arguments that can be used only for local function-systems.
    fn is_local(&self) -> bool;

    /// Returns access type performed on the entire [`World`].
    /// Most arguments will return some [`Access::Read`], and few will return none.
    fn world_access(&self) -> Option<Access>;

    /// Checks if this argument will skip specified archetype.
    fn skips_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns access type to the specified component type this argument may perform.
    fn access_component(&self, id: TypeId) -> Option<Access>;

    /// Returns access type to the specified resource type this argument may perform.
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

macro_rules! for_tuple {
    () => {
        for_tuple!(for A B C D E F G H I J K L M N O P Q R S T U V W X Y Z);
        // for_tuple!(for A);
    };

    (for) => {
        for_tuple!(impl);
    };

    (for $head:ident $($tail:ident)*) => {
        for_tuple!(for $($tail)*);
        for_tuple!(impl $head $($tail)*);
    };

    (impl $($a:ident)*) => {
        #[allow(unused_variables, unused_mut, non_snake_case)]
        unsafe impl<Func $(,$a)*> System for FunctionSystem<Func, ($($a,)*)>
        where
            $($a: FnArgCache,)*
            Func: for<'a> FnMut($(
                <$a as FnArgGet<'a>>::Arg,
            )*),
        {
            #[inline]
            fn is_local(&self) -> bool {
                let ($($a,)*) = &self.args;
                false $( || $a.is_local() )*
            }

            #[inline]
            fn world_access(&self) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut access = None;
                $(
                    access = merge_world_access(access, $a.world_access());
                )*
                access
            }

            #[inline]
            fn skips_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)*) = &self.args;
                false $( || $a.skips_archetype(archetype) )*
            }

            #[inline]
            fn access_component(&self, id: TypeId) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut access = None;
                $(
                    access = merge_access(access, $a.access_component(id));
                )*
                access
            }

            #[inline]
            fn access_resource(&self, id: TypeId) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut access = None;
                $(
                    access = merge_access(access, $a.access_resource(id));
                )*
                access
            }

            #[inline]
            unsafe fn run_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionQueue) {
                let ($($a,)*) = &mut self.args;

                {
                    $(
                        let $a = $a.get_unchecked(world, queue);
                    )*

                    (self.f)($($a,)*);
                }


                $(
                    $a.flush_unchecked(world, queue);
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

            #[inline]
            fn into_system(self) -> Self::System {
                FunctionSystem {
                    f: self,
                    args: ($($a::Cache::default(),)*),
                }
            }
        }
    }
}

for_tuple!();

/// Trait for values that can be created from [`World`] reference.
pub trait FromWorld {
    /// Returns new value created from [`World`] reference.
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

/// [`merge_access`] but panics when either argument is `Some(Access::Write)` and another is `Some(_)`.
#[inline]
const fn merge_world_access(lhs: Option<Access>, rhs: Option<Access>) -> Option<Access> {
    match (lhs, rhs) {
        (None, rhs) => rhs,
        (lhs, None) => lhs,
        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
        (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write)) => {
            panic!("Multiple mutable access to `World` is not allowed.");
        }
    }
}
