//! Provides a thread-local storage for the current world pointer.

use core::marker::PhantomData;
use std::{cell::Cell, ptr::NonNull};

use crate::{
    flow::{FlowEntity, FlowWorld},
    world::World,
    Entity, NoSuchEntity,
};

std::thread_local! {
    static WORLD_TLS: Cell<Option<NonNull<World>>> = const { Cell::new(None) };
}

/// Guard for setting and resetting the current world pointer.
pub struct Guard<'a> {
    #[cfg(debug_assertions)]
    this: NonNull<World>,
    prev: Option<NonNull<World>>,
    marker: PhantomData<&'a mut World>,
}

impl<'a> Guard<'a> {
    /// Sets the current world pointer.
    /// Keeps previous pointer and resets it back on drop.
    pub fn new(world: &'a mut World) -> Self {
        let this = NonNull::from(world);
        let prev = WORLD_TLS.with(|tls| tls.replace(Some(this)));
        Guard {
            #[cfg(debug_assertions)]
            this,
            prev,
            marker: PhantomData,
        }
    }

    pub fn world(&self) -> FlowWorld<'_> {
        unsafe { FlowWorld::make() }
    }

    pub fn entity(&self, entity: impl Entity) -> Result<FlowEntity<'_>, NoSuchEntity> {
        let id = entity.id();
        unsafe {
            if get_world().is_alive(id) {
                Ok(FlowEntity::make(id))
            } else {
                Err(NoSuchEntity)
            }
        }
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        WORLD_TLS.with(|tls| {
            #[cfg(debug_assertions)]
            assert_eq!(tls.get(), Some(self.this));
            tls.set(self.prev)
        });
    }
}

/// Returns the current world reference if set.
///
/// # Safety
///
/// Returns reference with unboud lifetime.
/// The caller is responsible to ensure that the reference
/// is not used after current `Guard` is dropped.
pub unsafe fn try_get_world<'a>() -> Option<&'a mut World> {
    WORLD_TLS.with(|tls| unsafe { tls.get().map(|mut w| w.as_mut()) })
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
pub unsafe fn get_world<'a>() -> &'a mut World {
    WORLD_TLS.with(|tls| unsafe { tls.get().unwrap_unchecked().as_mut() })
}
