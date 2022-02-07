use core::any::TypeId;

use crate::{
    archetype::Archetype,
    entity::{Entities, EntityId},
};

#[cfg(feature = "rc")]
use crate::entity::Entity;

use super::NoSuchEntity;

#[cfg(feature = "rc")]
use super::OwnershipError;

/// Meta-information about entities.
#[derive(Debug)]
pub struct EntityMeta<'a> {
    pub(super) entities: &'a mut Entities,
    pub(super) archetypes: &'a [Archetype],
}

impl EntityMeta<'_> {
    /// Transfers ownership of the entity from the caller to the `World`.
    /// After this call, entity won't be despawned until [`World::despawn`] is called with this entity id.
    #[cfg(feature = "rc")]
    pub fn keep<T>(&mut self, entity: Entity<T>) {
        assert!(self.entities.is_owner_of(&entity));
        self.entities.give_ownership(entity);
    }

    /// Transfers ownership of the entity from the `World` to the caller.
    /// After this call, entity should be despawned by dropping returned entity reference,
    /// or by returning ownership to the `World` and then called [`World::despawn`]
    ///
    /// Returns error if entity with specified id does not exists,
    /// or if that entity is not owned by the `World`.
    #[cfg(feature = "rc")]
    pub fn take(&mut self, entity: &EntityId) -> Result<Entity, OwnershipError> {
        self.entities.take_ownership(entity)
    }

    /// Checks if specified entity has componet of specified type.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn has_component_owning<T: 'static, U>(&self, entity: &Entity<U>) -> bool {
        assert!(self.entities.is_owner_of(entity));

        let (archetype, _idx) = self.entities.get(entity).unwrap();
        self.archetypes[archetype as usize].contains_id(TypeId::of::<T>())
    }

    /// Attemtps to check if specified entity has componet of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn has_component<T: 'static>(&self, entity: &EntityId) -> Result<bool, NoSuchEntity> {
        let (archetype, _idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;
        Ok(self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()))
    }

    /// Checks if specified entity is still alive.
    #[inline]
    pub fn is_alive(&self, entity: &EntityId) -> bool {
        self.entities.get(entity).is_some()
    }
}
