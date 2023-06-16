use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, ImmutablePhantomQuery, PhantomQuery, UnitFetch},
    relation::{OriginComponent, Relation},
};

phantom_newtype! {
    /// Filters origins of relation.
    pub struct FilterRelates<R>
}

impl<R> FilterRelates<R>
where
    R: Relation,
{
    /// Creates a new [`FilterRelates`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

unsafe impl<R> PhantomQuery for FilterRelates<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    const MUTABLE: bool = false;

    #[inline]
    fn access(_: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(_: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<R> ImmutablePhantomQuery for FilterRelates<R> where R: Relation {}

/// Returns a filter to filter origins of relation.
pub fn relates<R: Relation>() -> PhantomData<FilterRelates<R>> {
    PhantomData
}
