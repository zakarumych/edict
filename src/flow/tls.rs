//! Provides a thread-local storage for the current world pointer.

use core::{cell::Cell, marker::PhantomData, ptr::NonNull};

use crate::{entity::EntityId, world::WorldLocal};

#[cfg(feature = "std")]
std::thread_local! {
    static WORLD_TLS: Cell<Option<NonNull<WorldLocal>>> = const { Cell::new(None) };
    static ENTITY_TLS: Cell<Option<EntityId>> = const { Cell::new(None) };
}

#[cfg(feature = "std")]
fn set_edict_flow_world_tls(world: NonNull<WorldLocal>) -> Option<NonNull<WorldLocal>> {
    WORLD_TLS.with(|tls| tls.replace(Some(world)))
}

#[cfg(feature = "std")]
fn get_edict_flow_world_tls() -> Option<NonNull<WorldLocal>> {
    WORLD_TLS.with(|tls| tls.get())
}

#[cfg(feature = "std")]
fn reset_edict_flow_world_tls(prev: Option<NonNull<WorldLocal>>, world: NonNull<WorldLocal>) {
    WORLD_TLS.with(|tls| {
        assert_eq!(tls.get(), Some(world));
        tls.set(prev)
    });
}

#[cfg(feature = "std")]
fn set_edict_flow_entity_tls(entity: EntityId) -> Option<EntityId> {
    ENTITY_TLS.with(|tls| tls.replace(Some(entity)))
}

#[cfg(feature = "std")]
fn get_edict_flow_entity_tls() -> Option<EntityId> {
    ENTITY_TLS.with(|tls| tls.get())
}

#[cfg(feature = "std")]
fn reset_edict_flow_entity_tls(prev: Option<EntityId>, entity: EntityId) {
    ENTITY_TLS.with(|tls| {
        assert_eq!(tls.get(), Some(entity));
        tls.set(prev)
    });
}

#[cfg(not(feature = "std"))]
extern "C" {
    fn set_edict_flow_world_tls(world: NonNull<WorldLocal>);
    fn get_edict_flow_world_tls() -> Option<NonNull<WorldLocal>>;
    fn reset_edict_flow_world_tls(prev: Option<NonNull<WorldLocal>>, world: NonNull<WorldLocal>);

    fn set_edict_flow_entity_tls(entity: EntityId);
    fn get_edict_flow_entity_tls() -> Option<EntityId>;
    fn reset_edict_flow_entity_tls(prev: Option<EntityId>, entity: EntityId);
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
        let prev = set_edict_flow_world_tls(this);
        WorldGuard {
            this,
            prev,
            marker: PhantomData,
        }
    }
}

impl Drop for WorldGuard<'_> {
    fn drop(&mut self) {
        reset_edict_flow_world_tls(self.prev, self.this);
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
    get_edict_flow_world_tls().unwrap_unchecked().as_ref()
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
    get_edict_flow_world_tls().unwrap_unchecked().as_mut()
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
        let prev = set_edict_flow_entity_tls(entity);
        EntityGuard { entity, prev }
    }
}

impl Drop for EntityGuard {
    fn drop(&mut self) {
        reset_edict_flow_entity_tls(self.prev, self.entity);
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
    get_edict_flow_entity_tls()
}
