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
pub trait QueryArgCache: for<'a> QueryArgGet<'a> + Send + Default + 'static {
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
        let world = world.as_ref(); // # Safety: Declares read access.
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

macro_rules! for_tuple {
    () => {
        // This one is shorter because `Default` is not implemented for larger tuples.
        for_tuple!(for A B C D E F G H I J L M);
        // for_tuple!(for A);
    };

    (for) => {
        for_tuple!(impl);
    };

    (for $head:ident $($tail:ident)*) => {
        for_tuple!(for $($tail)*);
        for_tuple!(impl $head $($tail)*);
    };

    (impl) => {
        impl<'a> QueryArgGet<'a> for () {
            type Arg = ();
            type Query = ();

            #[inline]
            #[must_use]
            fn get(&mut self, _world: &World) {}
        }

        impl QueryArgCache for () {
            #[inline]
            fn visit_archetype(&self, _archetype: &Archetype) -> bool {
                true
            }
            #[inline]
            fn access_component(&self, _id: TypeId) -> Option<Access> {
                None
            }
        }

        impl QueryArg for () {
            type Cache = ();
        }
    };

    (impl $($a:ident)+) => {
        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        impl<'a $(, $a)+> QueryArgGet<'a> for ($($a,)+)
        where
            $($a: QueryArgGet<'a>,)+
        {
            type Arg = ($($a::Arg,)+);
            type Query = ($($a::Query,)+);

            #[inline]
            fn get(&'a mut self, world: &'a World) -> Self::Query {
                let ($($a,)+) = self;
                ($($a::get($a, world),)+)
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        impl<$($a),+> QueryArgCache for ($($a,)+)
        where
            $($a: QueryArgCache,)+
        {
            #[inline]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)+) = self;
                $($a.visit_archetype(archetype))&&+
            }

            #[inline]
            fn access_component(&self, _id: TypeId) -> Option<Access> {
                let ($($a,)+) = self;
                let mut access = None;
                $({
                    access = merge_access(access, $a.access_component(_id));
                })*
                access
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        impl<$($a),+> QueryArg for ($($a,)+)
        where
            $($a: QueryArg,)+
        {
            type Cache = ($($a::Cache,)+);
        }
    };
}

for_tuple!();
