//! Provides a thread-local storage for the current world pointer.

use core::{marker::PhantomData, ptr::NonNull};

use crate::{entity::EntityId, world::WorldLocal};

#[cfg(not(feature = "std"))]
use crate::nostd::flow::{
    edict_get_flow_entity_tls, edict_get_flow_world_tls, edict_reset_flow_entity_tls,
    edict_reset_flow_world_tls, edict_set_flow_entity_tls, edict_set_flow_world_tls,
};

#[cfg(feature = "std")]
std::thread_local! {
    static WORLD_TLS: std::cell::Cell<Option<NonNull<WorldLocal>>> = const { std::cell::Cell::new(None) };
    static ENTITY_TLS: std::cell::Cell<Option<EntityId>> = const { std::cell::Cell::new(None) };
}

/// Guard for setting and resetting the current world pointer.
pub struct WorldGuard<'a> {
    this: NonNull<WorldLocal>,
    prev: Option<NonNull<WorldLocal>>,
    marker: PhantomData<&'a mut WorldLocal>,
}

impl<'a> WorldGuard<'a> {
    /// Sets the current world pointer.
    /// Keeps previous pointer and resets it back on drop.
    pub fn new(world: &'a mut WorldLocal) -> Self {
        let this = NonNull::from(world);

        #[cfg(feature = "std")]
        let prev = WORLD_TLS.with(|cell| cell.replace(Some(this)));

        #[cfg(not(feature = "std"))]
        let prev = unsafe { edict_set_flow_world_tls(this.cast()).map(NonNull::cast) };

        WorldGuard {
            this,
            prev,
            marker: PhantomData,
        }
    }
}

impl Drop for WorldGuard<'_> {
    fn drop(&mut self) {
        #[cfg(feature = "std")]
        WORLD_TLS.with(|cell| {
            let top = cell.replace(self.prev);
            debug_assert_eq!(top, Some(self.this));
        });

        #[cfg(not(feature = "std"))]
        unsafe {
            edict_reset_flow_world_tls(self.prev.map(NonNull::cast), self.this.cast());
        }
    }
}

/// Returns the current world reference.
///
/// # Safety
///
/// `Guard` must exist.
///
/// Returns reference with unboud lifetime.
/// The caller is responsible to ensure that the reference
/// is not used after current `Guard` is dropped.
pub(super) unsafe fn get_world_ref<'a>() -> &'a WorldLocal {
    #[cfg(feature = "std")]
    let world = WORLD_TLS.with(|cell| cell.get());

    #[cfg(not(feature = "std"))]
    let world = edict_get_flow_world_tls().map(NonNull::cast);

    world.unwrap().as_ref()
}

/// Returns the current world reference.
///
/// # Safety
///
/// `WorldGuard` must exist.
///
/// Returns reference with unboud lifetime.
/// The caller is responsible to ensure that the reference
/// is not used after current `Guard` is dropped.
pub(super) unsafe fn get_world_mut<'a>() -> &'a mut WorldLocal {
    #[cfg(feature = "std")]
    let world = WORLD_TLS.with(|cell| cell.get());

    #[cfg(not(feature = "std"))]
    let world = edict_get_flow_world_tls().map(NonNull::cast);

    world.unwrap().as_mut()
}

/// Guard for setting and resetting the current world pointer.
pub struct EntityGuard {
    entity: EntityId,
    prev: Option<EntityId>,
}

impl EntityGuard {
    /// Sets the current world pointer.
    /// Keeps previous pointer and resets it back on drop.
    pub fn new(entity: EntityId) -> Self {
        #[cfg(feature = "std")]
        let prev = ENTITY_TLS.with(|cell| cell.replace(Some(entity)));

        #[cfg(not(feature = "std"))]
        let prev = unsafe { edict_set_flow_entity_tls(entity) };
        EntityGuard { entity, prev }
    }
}

impl Drop for EntityGuard {
    fn drop(&mut self) {
        #[cfg(feature = "std")]
        ENTITY_TLS.with(|cell| {
            let top = cell.replace(self.prev);
            debug_assert_eq!(top, Some(self.entity));
        });

        #[cfg(not(feature = "std"))]
        unsafe {
            edict_reset_flow_entity_tls(self.prev, self.entity);
        }
    }
}

/// Returns the current enitty id.
///
/// # Safety
///
/// `Guard` must exist.
///
/// Returns reference with unboud lifetime.
/// The caller is responsible to ensure that the reference
/// is not used after current `Guard` is dropped.
pub(super) fn get_entity() -> Option<EntityId> {
    #[cfg(feature = "std")]
    let entity = ENTITY_TLS.with(|cell| cell.get());

    #[cfg(not(feature = "std"))]
    let entity = unsafe { edict_get_flow_entity_tls() };

    entity
}
