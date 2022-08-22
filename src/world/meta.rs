use core::any::TypeId;

use crate::{
    archetype::Archetype,
    entity::{Entities, EntityId},
};

use super::NoSuchEntity;

/// Meta-information about entities.
pub struct EntityMeta<'a> {
    pub(super) entities: &'a Entities,
    pub(super) archetypes: &'a [Archetype],
}

impl EntityMeta<'_> {
    /// Checks if specified entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn has_component<T: 'static>(&self, entity: EntityId) -> Result<bool, NoSuchEntity> {
        let (archetype, _idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;
        Ok(self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()))
    }

    /// Checks if specified entity is still alive.
    #[inline]
    pub fn is_alive(&self, entity: EntityId) -> bool {
        self.entities.get(entity).is_some()
    }
}
