use core::any::TypeId;

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    entity::{EntityId, EntityLoc, Location},
    system::QueryArg,
};

use super::{
    Access, AsQuery, BatchFetch, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery,
    WriteAlias,
};

/// [`Fetch`] type for the [`Entities`] query.
pub struct EntitiesFetch<'a> {
    archetype: u32,
    entities: &'a [EntityId],
}

unsafe impl<'a> Fetch<'a> for EntitiesFetch<'a> {
    type Item = EntityLoc<'a>;

    #[inline]
    fn dangling() -> Self {
        EntitiesFetch {
            archetype: u32::MAX,
            entities: &[],
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> EntityLoc<'a> {
        let id = unsafe { *self.entities.get_unchecked(idx as usize) };
        EntityLoc::from_parts(id, Location::new(self.archetype, idx))
    }
}

unsafe impl<'a> BatchFetch<'a> for EntitiesFetch<'a> {
    type Batch = &'a [EntityId];

    #[inline]
    unsafe fn get_batch(&mut self, start: u32, end: u32) -> &'a [EntityId] {
        debug_assert!(end >= start);

        unsafe { &*self.entities.get_unchecked(start as usize..end as usize) }
    }
}

marker_type! {
    /// Queries entity ids.
    pub struct Entities;
}

impl AsQuery for Entities {
    type Query = Self;
}

impl IntoQuery for Entities {
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl DefaultQuery for Entities {
    #[inline]
    fn default_query() -> Self {
        Entities
    }
}

impl QueryArg for Entities {
    #[inline]
    fn new() -> Self {
        Entities
    }
}

unsafe impl Query for Entities {
    type Fetch<'a> = EntitiesFetch<'a>;
    type Item<'a> = EntityLoc<'a>;

    const MUTABLE: bool = false;

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }

    #[inline]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

    #[inline]
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

    #[inline]
    fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<EntityLoc<'a>>
    where
        EntityId: 'a,
    {
        Some(EntityLoc::from_parts(id, Location::reserved(idx)))
    }
}

unsafe impl ImmutableQuery for Entities {}
unsafe impl SendQuery for Entities {}
