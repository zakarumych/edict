use core::{any::TypeId, ptr::NonNull};

use crate::{
    archetype::Archetype,
    query::{merge_access, Access, Query},
    system::ActionQueue,
    view::{View, ViewValue},
    world::World,
};

use super::{FnArg, FnArgState};

/// State for an argument that is stored between calls to function-system.
pub trait QueryArgState: Send + 'static {
    /// Argument specified in [`View`]
    type Arg<'a>: QueryArg<State = Self>;

    /// Constructs new cache instance.
    #[must_use]
    fn new() -> Self;

    /// Returns true if the query visits archetype.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns some access type performed by the query.
    #[must_use]
    fn access_component(&self, id: TypeId) -> Option<Access>;

    /// Returns query for an argument.
    #[must_use]
    fn get<'a>(&'a mut self, world: &'a World) -> Self::Arg<'a>;
}

/// Types that can be used as queries within [`View`] args for function-systems.
pub trait QueryArg: Query {
    /// State for the query that is stored between calls to function-system.
    type State: QueryArgState;
}

/// State type used by corresponding [`QueryRef`].
#[derive(Default)]
pub struct ViewState<Q, F> {
    query: Q,
    filter: F,
}

unsafe impl<Q, F> FnArgState for ViewState<Q, F>
where
    Q: QueryArgState,
    F: QueryArgState,
{
    type Arg<'a> = View<'a, Q::Arg<'a>, F::Arg<'a>>;

    #[inline(always)]
    fn new() -> Self {
        ViewState {
            query: Q::new(),
            filter: F::new(),
        }
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        false
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.query.visit_archetype(archetype) && self.filter.visit_archetype(archetype)
    }

    #[inline(always)]
    fn access_component(&self, id: TypeId) -> Option<Access> {
        merge_access::<Self>(
            self.query.access_component(id),
            self.filter.access_component(id),
        )
    }

    #[inline(always)]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> View<'a, Q::Arg<'a>, F::Arg<'a>> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        let query = self.query.get(world);
        let filter = self.filter.get(world);
        ViewValue::new(world, query, filter)
    }
}

impl<'a, Q, F, B> FnArg for ViewValue<'a, Q, F, B>
where
    Q: QueryArg,
    F: QueryArg,
{
    type State = ViewState<Q::State, F::State>;
}

macro_rules! impl_query {
    ($($a:ident)*) => {
        #[allow(non_snake_case)]
        #[allow(unused_parens, unused_variables, unused_mut)]
        impl<$($a,)*> QueryArgState for ($($a,)*)
        where
            $($a: QueryArgState,)*
        {
            type Arg<'a> = ($($a::Arg<'a>,)*);

            #[inline(always)]
            fn new() -> Self {
                ($($a::new(),)*)
            }

            #[inline(always)]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)*) = self;
                true $(&& $a.visit_archetype(archetype))*
            }

            #[inline(always)]
            fn access_component(&self, _id: TypeId) -> Option<Access> {
                let ($($a,)*) = self;
                let mut access = None;
                $({
                    access = merge_access::<Self>(access, $a.access_component(_id));
                })*
                access
            }

            #[inline(always)]
            fn get<'a>(&'a mut self, world: &'a World) -> Self::Arg<'a> {
                let ($($a,)*) = self;
                ($($a::get($a, world),)*)
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        impl<$($a),*> QueryArg for ($($a,)*)
        where
            $($a: QueryArg,)*
        {
            type State = ($($a::State,)*);
        }
    };
}

for_tuple!(impl_query);

#[test]
fn test_system() {
    fn foo(_: View<&u32>) {}
}
