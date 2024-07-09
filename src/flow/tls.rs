//! Provides a thread-local storage for the current world pointer.

use core::{marker::PhantomData, ptr::NonNull};

use crate::world::WorldLocal;

#[cfg(not(feature = "std"))]
use crate::nostd::flow::{
    edict_get_flow_world_tls, edict_reset_flow_world_tls, edict_set_flow_world_tls,
};

#[cfg(feature = "std")]
std::thread_local! {
    static WORLD_TLS: std::cell::Cell<Option<NonNull<WorldLocal>>> = const { std::cell::Cell::new(None) };
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

    unsafe { world.unwrap().as_mut() }
}
