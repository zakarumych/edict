use core::{any::TypeId, ptr::NonNull};

use crate::{
    archetype::Archetype,
    query::{merge_access, Access, IntoQuery, Query},
    system::ActionQueue,
    world::{QueryRef, World},
};

use super::{FnArg, FnArgCache, FnArgGet};

/// Cache for an argument that is stored between calls to function-system.
pub trait QueryArgGet<'a> {
    /// Argument specified in [`QueryRef`]
    type Arg: QueryArg<Cache = Self, Query = Self::Query>;

    /// Constructed query type.
    type Query: Query;

    /// Returns query for an argument.
    #[must_use]
    fn get(&'a mut self, world: &'a World) -> Self::Query;
}

/// Cache for an argument that is stored between calls to function-system.
pub trait QueryArgCache: for<'a> QueryArgGet<'a> + Send + 'static {
    /// Constructs new cache instance.
    #[must_use]
    fn new() -> Self;

    /// Returns true if the query visits archetype.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns some access type performed by the query.
    #[must_use]
    fn access_component(&self, id: TypeId) -> Option<Access>;
}

/// Types that can be used as queries with [`QueryRef`] for function-systems.
pub trait QueryArg: IntoQuery {
    /// Cache for an argument that is stored between calls to function-system.
    type Cache: QueryArgCache;
}

/// Cache type used by corresponding [`QueryRef`].
#[derive(Default)]
pub struct QueryRefCache<Q, F> {
    query: Q,
    filter: F,
}

unsafe impl<'a, Q, F> FnArgGet<'a> for QueryRefCache<Q, F>
where
    Q: QueryArgCache,
    F: QueryArgCache,
{
    type Arg = QueryRef<'a, <Q as QueryArgGet<'a>>::Arg, <F as QueryArgGet<'a>>::Arg>;

    #[inline]
    unsafe fn get_unchecked(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> Self::Arg {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        let query = self.query.get(world);
        let filter = self.filter.get(world);
        QueryRef::new(world, query, filter)
    }
}

impl<Q, F> FnArgCache for QueryRefCache<Q, F>
where
    Q: QueryArgCache,
    F: QueryArgCache,
{
    #[inline]
    fn new() -> Self {
        QueryRefCache {
            query: Q::new(),
            filter: F::new(),
        }
    }

    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.query.visit_archetype(archetype) && self.filter.visit_archetype(archetype)
    }

    #[inline]
    fn access_component(&self, id: TypeId) -> Option<Access> {
        merge_access(
            self.query.access_component(id),
            self.filter.access_component(id),
        )
    }

    #[inline]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        None
    }
}

impl<'a, Q, F> FnArg for QueryRef<'a, Q, F>
where
    Q: QueryArg,
    F: QueryArg,
{
    type Cache = QueryRefCache<Q::Cache, F::Cache>;
}

macro_rules! impl_query {
    ($($a:ident)*) => {
        #[allow(non_snake_case)]
        #[allow(unused_parens, unused_variables)]
        impl<'a $(, $a)*> QueryArgGet<'a> for ($($a,)*)
        where
            $($a: QueryArgGet<'a>,)*
        {
            type Arg = ($($a::Arg,)*);
            type Query = ($($a::Query,)*);

            #[inline]
            fn get(&'a mut self, world: &'a World) -> Self::Query {
                let ($($a,)*) = self;
                ($($a::get($a, world),)*)
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens, unused_variables, unused_mut)]
        impl<$($a,)*> QueryArgCache for ($($a,)*)
        where
            $($a: QueryArgCache,)*
        {
            #[inline]
            fn new() -> Self {
                ($($a::new(),)*)
            }

            #[inline]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)*) = self;
                true $(&& $a.visit_archetype(archetype))*
            }

            #[inline]
            fn access_component(&self, _id: TypeId) -> Option<Access> {
                let ($($a,)*) = self;
                let mut access = None;
                $({
                    access = merge_access(access, $a.access_component(_id));
                })*
                access
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        impl<$($a),*> QueryArg for ($($a,)*)
        where
            $($a: QueryArg,)*
        {
            type Cache = ($($a::Cache,)*);
        }
    };
}

for_tuple!(impl_query);
