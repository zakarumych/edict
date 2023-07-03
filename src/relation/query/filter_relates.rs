use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, DefaultQuery, ImmutableQuery, IntoQuery, Query, UnitFetch},
    relation::{OriginComponent, Relation},
};

marker_type! {
    /// Filters origins of relation.
    pub struct FilterRelates<R>;
}

impl<R> IntoQuery for FilterRelates<R>
where
    R: Relation,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for FilterRelates<R>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        FilterRelates
    }
}

unsafe impl<R> Query for FilterRelates<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn access(&self, _: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

    #[inline(always)]
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<R> ImmutableQuery for FilterRelates<R> where R: Relation {}

/// Returns a filter to filter origins of relation.
pub fn relates<R: Relation>() -> PhantomData<FilterRelates<R>> {
    PhantomData
}
