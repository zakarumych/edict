//! Provides a thread-local storage for the current world pointer.

use core::marker::PhantomData;
use std::{cell::Cell, ptr::NonNull};

use crate::world::WorldLocal;

std::thread_local! {
    static WORLD_TLS: Cell<Option<NonNull<WorldLocal>>> = const { Cell::new(None) };
}

/// Guard for setting and resetting the current world pointer.
pub struct Guard<'a> {
    #[cfg(debug_assertions)]
    this: NonNull<WorldLocal>,
    prev: Option<NonNull<WorldLocal>>,
    marker: PhantomData<&'a mut WorldLocal>,
}

impl<'a> Guard<'a> {
    /// Sets the current world pointer.
    /// Keeps previous pointer and resets it back on drop.
    pub fn new(world: &'a mut WorldLocal) -> Self {
        let this = NonNull::from(world);
        let prev = WORLD_TLS.with(|tls| tls.replace(Some(this)));
        Guard {
            #[cfg(debug_assertions)]
            this,
            prev,
            marker: PhantomData,
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
pub unsafe fn try_get_world_ref<'a>() -> Option<&'a WorldLocal> {
    WORLD_TLS.with(|tls| unsafe { tls.get().map(|w| w.as_ref()) })
}

/// Returns the current world reference if set.
///
/// # Safety
///
/// Returns reference with unboud lifetime.
/// The caller is responsible to ensure that the reference
/// is not used after current `Guard` is dropped.
pub unsafe fn try_get_world_mut<'a>() -> Option<&'a mut WorldLocal> {
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
pub unsafe fn get_world_ref<'a>() -> &'a WorldLocal {
    WORLD_TLS.with(|tls| unsafe { tls.get().unwrap_unchecked().as_ref() })
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
pub unsafe fn get_world_mut<'a>() -> &'a mut WorldLocal {
    WORLD_TLS.with(|tls| unsafe { tls.get().unwrap_unchecked().as_mut() })
}
