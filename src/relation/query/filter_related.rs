use core::any::TypeId;

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{DefaultQuery, ImmutableQuery, IntoQuery, Query, UnitFetch, WriteAlias},
    relation::{Relation, TargetComponent},
    system::QueryArg,
    Access,
};

marker_type! {
    /// Filters targets of relation.
    pub struct FilterRelated<R>;
}

impl<R> IntoQuery for FilterRelated<R>
where
    R: Relation,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for FilterRelated<R>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        FilterRelated
    }
}

impl<R> QueryArg for FilterRelated<R>
where
    R: Relation,
{
    #[inline(always)]
    fn new() -> Self {
        FilterRelated
    }
}

unsafe impl<R> Query for FilterRelated<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_type_access(&self, _: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<TargetComponent<R>>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

    #[inline(always)]
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<R> ImmutableQuery for FilterRelated<R> where R: Relation {}
