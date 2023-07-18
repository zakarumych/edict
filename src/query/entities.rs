use core::any::TypeId;

use crate::{
    archetype::Archetype,
    entity::{EntityId, EntityLoc, Location},
    system::QueryArg,
};

use super::{Access, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, WriteAlias};

/// [`Fetch`] type for the [`Entities`] query.
pub struct EntitiesFetch<'a> {
    archetype: u32,
    entities: &'a [EntityId],
}

unsafe impl<'a> Fetch<'a> for EntitiesFetch<'a> {
    type Item = EntityLoc<'a>;

    #[inline(always)]
    fn dangling() -> Self {
        EntitiesFetch {
            archetype: u32::MAX,
            entities: &[],
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> EntityLoc<'a> {
        let id = *self.entities.get_unchecked(idx as usize);
        EntityLoc::new(id, Location::new(self.archetype, idx))
    }
}

marker_type! {
    /// Queries entity ids.
    pub struct Entities;
}

impl IntoQuery for Entities {
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl DefaultQuery for Entities {
    #[inline(always)]
    fn default_query() -> Self {
        Entities
    }
}

impl QueryArg for Entities {
    #[inline(always)]
    fn new() -> Self {
        Entities
    }
}

unsafe impl Query for Entities {
    type Fetch<'a> = EntitiesFetch<'a>;
    type Item<'a> = EntityLoc<'a>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_type_access(&self, _ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }

    #[inline(always)]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: crate::epoch::EpochId,
    ) -> EntitiesFetch<'a> {
        EntitiesFetch {
            archetype: arch_idx,
            entities: archetype.entities(),
        }
    }

    #[inline(always)]
    fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<EntityLoc<'a>>
    where
        EntityId: 'a,
    {
        Some(EntityLoc::new(id, Location::reserved(idx)))
    }
}

unsafe impl ImmutableQuery for Entities {}
