//! Provides API to define systems compatible with built-in scheduler.

use core::{
    any::TypeId,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{archetype::Archetype, query::merge_access, query::Access, world::World};

/// System trait that should be implemented by systems in order to be added to the [`Scheduler`].
/// Systems run with context `C` that is shared between all systems.
pub trait System {
    /// Checks if all queries from this system will skip specified archetype.
    fn skips_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns access type to the specified component type this system may perform.
    fn access_component(&self, id: TypeId) -> Option<Access>;

    /// Returns access type to the specified resource type this system may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access>;

    /// Runs the system with given context instance.
    fn run(&mut self, world: &World);
}

/// Trait for types that can be converted into systems.
pub trait IntoSystem<Marker> {
    /// Type of the system a value of this type can be converted into.
    type System: System + 'static;

    /// Converts value into system.
    fn into_system(self) -> Self::System;
}

/// Marker for [`IntoSystem`] for functions.
pub struct IsFunctionSystem<Args> {
    marker: PhantomData<fn(Args)>,
}

/// State for an argument that is stored between calls to function-system.
pub trait FnArgState<'a>: Default {
    /// Argument supplied to function-system.
    type Arg: 'a;

    /// Checks if this argument will skip specified archetype.
    fn skips_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns access type to the specified component type this argument may perform.
    fn access_component(&self, id: TypeId) -> Option<Access>;

    /// Returns access type to the specified resource type this argument may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access>;

    /// Extracts argument from the world.
    fn extract(&'a mut self, world: &'a World) -> Self::Arg;
}

/// Types that can be used as an argument for function-systems.
pub trait FnSystemArg {
    /// State for an argument that is stored between calls to function-system.
    type State: for<'a> FnArgState<'a> + 'static;
}

/// Wrapper for function-like values and implements [`System`].
pub struct FunctionSystem<F, Args> {
    f: F,
    args: Args,
}

// impl<F, A> System for FunctionSystem<F, (A,)>
// where
//     A: for<'a> FnArgState<'a>,
//     F: for<'a> FnMut(<A as FnArgState<'a>>::Arg),
// {
//     fn skips_archetype(&self, archetype: &Archetype) -> bool {
//         let (a,) = &self.args;
//         a.skips_archetype(archetype)
//     }
//     fn access_component(&self, id: TypeId) -> Option<Access> {
//         let (a,) = &self.args;
//         a.access_component(id)
//     }
//     fn access_resource(&self, id: TypeId) -> Option<Access> {
//         let (a,) = &self.args;
//         a.access_resource(id)
//     }
//     fn run(&mut self, world: &World) {
//         let (a,) = &mut self.args;

//         let a = a.extract(world);

//         (self.f)(a);
//     }
// }

// impl<A, F> IntoSystem<(A,), IsFunctionSystem> for F
// where
//     A: FnSystemArg,
//     F: FnMut(A) + for<'a> FnMut(<A::State as FnArgState<'a>>::Arg),
// {
//     type System = FunctionSystem<Self, (A::State,)>;

//     fn into_system(self) -> FunctionSystem<Self, (A::State,)> {
//         FunctionSystem {
//             f: self,
//             args: (A::State::default(),),
//         }
//     }
// }

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
        impl<Func $(,$a)*> System for FunctionSystem<Func, ($($a,)*)>
        where
            $($a: for<'a> FnArgState<'a>,)*
            Func: for<'a> FnMut($(
                <$a as FnArgState<'a>>::Arg,
            )*),
        {
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
            fn run(&mut self, world: &World) {
                let ($($a,)*) = &mut self.args;

                $(
                    let $a = $a.extract(world);
                )*

                (self.f)($($a,)*);
            }
        }

        impl<Func $(, $a)*> IntoSystem<IsFunctionSystem<($($a,)*)>> for Func
        where
            $($a: FnSystemArg,)*
            Func: FnMut($($a,)*) + 'static,
            Func: for<'a> FnMut($(
                <$a::State as FnArgState<'a>>::Arg,
            )*),
        {
            type System = FunctionSystem<Self, ($($a::State,)*)>;

            fn into_system(self) -> Self::System {
                FunctionSystem {
                    f: self,
                    args: ($($a::State::default(),)*),
                }
            }
        }
    }
}

for_tuple!();

/// Bare state for function systems.
#[derive(Default)]
#[repr(transparent)]
pub struct State<T> {
    value: T,
}

impl<T> Deref for State<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for State<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> FnSystemArg for &mut State<T>
where
    T: Default + 'static,
{
    type State = State<T>;
}

impl<'a, T> FnArgState<'a> for State<T>
where
    T: Default + 'a,
{
    type Arg = &'a mut Self;

    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        None
    }
    fn extract(&'a mut self, _world: &'_ World) -> &'a mut Self {
        self
    }
}

#[test]
fn test_system() {
    fn foo() {}
    fn bar(_: &mut State<u32>) {}

    let foo_system = foo.into_system();
    let bar_system = bar.into_system();
}
