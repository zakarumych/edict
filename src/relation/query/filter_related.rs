use core::any::TypeId;

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        AsQuery, DefaultQuery, ImmutableQuery, IntoQuery, Query, SendQuery, UnitFetch, WriteAlias,
    },
    relation::{OriginComponent, Relation, TargetComponent},
    system::QueryArg,
    type_id, Access,
};

marker_type! {
    /// Filters targets of relation.
    pub struct FilterRelated<R>;
}

impl<R> AsQuery for FilterRelated<R>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for FilterRelated<R>
where
    R: Relation,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for FilterRelated<R>
where
    R: Relation,
{
    #[inline]
    fn default_query() -> Self {
        FilterRelated
    }
}

impl<R> QueryArg for FilterRelated<R>
where
    R: Relation,
{
    #[inline]
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

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        if R::SYMMETRIC {
            archetype.has_component(type_id::<OriginComponent<R>>())
        } else {
            archetype.has_component(type_id::<TargetComponent<R>>())
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<R> ImmutableQuery for FilterRelated<R> where R: Relation {}
unsafe impl<R> SendQuery for FilterRelated<R> where R: Relation {}
