mod query;
mod res;
mod state;

use core::{any::TypeId, marker::PhantomData};

use super::{IntoSystem, System};
use crate::{
    archetype::Archetype,
    query::{merge_access, Access},
    world::World,
};

pub use self::{
    query::{QueryArg, QueryArgCache, QueryArgGet, QueryRefCache},
    state::{State, StateCache},
    res::{Res, ResMut, ResMutCache, ResCache, ResNoSync, ResNoSyncCache, ResMutNoSend, ResMutNoSendCache},
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
    unsafe fn get_unchecked(&'a mut self, world: &'a World) -> Self::Arg;
}

/// Cache for an argument that is stored between calls to function-system.
///
/// # Safety
///
/// If [`FnArgFetch::is_local`] returns false [`FnArgFetch::extract_unchecked`] must be safe to call from any thread.
/// Otherwise [`FnArgFetch::extract_unchecked`] must be safe to call from local thread.
pub trait FnArgCache: for<'a> FnArgGet<'a> + Default + 'static {
    /// Returns `true` for local arguments that can be used only for local function-systems.
    fn is_local(&self) -> bool;

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
            fn is_local(&self) -> bool {
                let ($($a,)*) = &self.args;
                false $( || $a.is_local() )*
            }

            fn skips_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)*) = &self.args;
                false $( || $a.skips_archetype(archetype) )*
            }

            fn access_component(&self, id: TypeId) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut access = None;
                $(
                    access = merge_access(access, $a.access_component(id));
                )*
                access
            }
            fn access_resource(&self, id: TypeId) -> Option<Access> {
                let ($($a,)*) = &self.args;
                let mut access = None;
                $(
                    access = merge_access(access, $a.access_resource(id));
                )*
                access
            }

            unsafe fn run_unchecked(&mut self, world: &World) {
                let ($($a,)*) = &mut self.args;

                $(
                    let $a = $a.get_unchecked(world);
                )*

                (self.f)($($a,)*);
            }
        }

        impl<Func $(, $a)*> IntoSystem<IsFunctionSystem<($($a,)*)>> for Func
        where
            $($a: FnArg,)*
            $($a::Cache: Send + Sync,)*
            Func: FnMut($($a,)*) + Send + Sync + 'static,
            Func: for<'a> FnMut($(
                <$a::Cache as FnArgGet<'a>>::Arg,
            )*),
        {
            type System = FunctionSystem<Self, ($($a::Cache,)*)>;

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
    fn from_world(_: &World) -> Self {
        T::default()
    }
}
