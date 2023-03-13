use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, ImmutablePhantomQuery, IntoQuery, PhantomQuery, UnitFetch},
    relation::{Relation, TargetComponent},
};

phantom_newtype! {
    /// Filters targets of relation.
    pub struct FilterRelated<R>
}

impl<R> FilterRelated<R>
where
    R: Relation,
{
    /// Creates a new [`FilterRelated`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

impl<R> IntoQuery for FilterRelated<R>
where
    R: Relation,
{
    type Query = PhantomData<fn() -> FilterRelated<R>>;
}

unsafe impl<R> PhantomQuery for FilterRelated<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    #[inline]
    fn access(_: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<TargetComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(_: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<R> ImmutablePhantomQuery for FilterRelated<R> where R: Relation {}

/// Returns a filter to filter targets of relation.
pub fn related<R: Relation>() -> PhantomData<FilterRelated<R>> {
    PhantomData
}
