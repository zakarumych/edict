use core::any::TypeId;

use crate::{
    Access,
    archetype::Archetype,
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        AsQuery, DefaultQuery, ImmutableQuery, IntoQuery, Query, SendQuery, UnitFetch, WriteAlias,
    },
    relation::{OriginComponent, Relation},
    system::DefaultQueryArg,
    type_id,
};

marker_type! {
    /// Filters origins of relation.
    pub struct FilterRelates<R>;
}

impl<R> AsQuery for FilterRelates<R>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for FilterRelates<R>
where
    R: Relation,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for FilterRelates<R>
where
    R: Relation,
{
    #[inline]
    fn default_query() -> Self {
        FilterRelates
    }
}

impl<R> DefaultQueryArg for FilterRelates<R> where R: Relation {}

unsafe impl<R> Query for FilterRelates<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    const MUTABLE: bool = false;

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }

    #[inline]
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        Some(archetype.has_component(type_id::<OriginComponent<R>>()))
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(type_id::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<R> ImmutableQuery for FilterRelates<R> where R: Relation {}
unsafe impl<R> SendQuery for FilterRelates<R> where R: Relation {}
