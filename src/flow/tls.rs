//! Thread-local flow context and exclusive safe access to its world.

use crate::world::WorldLocal;
use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

#[cfg(not(feature = "std"))]
use crate::nostd::flow::{
    edict_get_flow_world_tls, edict_replace_flow_world_borrowed_tls, edict_reset_flow_world_tls,
    edict_set_flow_world_tls,
};

#[cfg(feature = "std")]
std::thread_local! {
    static WORLD_TLS: std::cell::Cell<Option<NonNull<WorldLocal>>> = const { std::cell::Cell::new(None) };
    static BORROWED_TLS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn replace_borrowed(value: bool) -> bool {
    #[cfg(feature = "std")]
    {
        BORROWED_TLS.with(|cell| cell.replace(value))
    }
    #[cfg(not(feature = "std"))]
    {
        unsafe { edict_replace_flow_world_borrowed_tls(value) }
    }
}

struct BorrowFlagGuard;
impl BorrowFlagGuard {
    fn acquire() -> Self {
        assert!(!replace_borrowed(true), "flow world is already borrowed");
        Self
    }
}
impl Drop for BorrowFlagGuard {
    fn drop(&mut self) {
        let was_borrowed = replace_borrowed(false);
        debug_assert!(was_borrowed);
    }
}

/// Exclusive access to the current flow world. References cannot outlive this guard.
pub(super) struct WorldBorrowGuard {
    world: NonNull<WorldLocal>,
    _flag: BorrowFlagGuard,
    _local: PhantomData<*mut ()>,
}
impl Deref for WorldBorrowGuard {
    type Target = WorldLocal;
    fn deref(&self) -> &WorldLocal {
        unsafe { self.world.as_ref() }
    }
}
impl DerefMut for WorldBorrowGuard {
    fn deref_mut(&mut self) -> &mut WorldLocal {
        unsafe { self.world.as_mut() }
    }
}

pub(super) fn borrow_world() -> WorldBorrowGuard {
    // Acquire before creating any world reference, including on reentrant calls.
    let flag = BorrowFlagGuard::acquire();
    let world = current_world().expect("flow world is not bound on this thread");
    WorldBorrowGuard {
        world,
        _flag: flag,
        _local: PhantomData,
    }
}

fn current_world() -> Option<NonNull<WorldLocal>> {
    #[cfg(feature = "std")]
    {
        WORLD_TLS.with(|cell| cell.get())
    }
    #[cfg(not(feature = "std"))]
    {
        unsafe { edict_get_flow_world_tls() }.map(NonNull::cast)
    }
}

/// Guard for setting and resetting the current world pointer.
pub struct WorldGuard<'a> {
    this: NonNull<WorldLocal>,
    prev: Option<NonNull<WorldLocal>>,
    marker: PhantomData<&'a mut WorldLocal>,
}
impl<'a> WorldGuard<'a> {
    pub fn new(world: &'a mut WorldLocal) -> Self {
        let flag = BorrowFlagGuard::acquire();
        let this = NonNull::from(world);
        #[cfg(feature = "std")]
        let prev = WORLD_TLS.with(|cell| cell.replace(Some(this)));
        #[cfg(not(feature = "std"))]
        let prev = unsafe { edict_set_flow_world_tls(this.cast()).map(NonNull::cast) };
        let guard = WorldGuard {
            this,
            prev,
            marker: PhantomData,
        };
        drop(flag);
        guard
    }
}
impl Drop for WorldGuard<'_> {
    fn drop(&mut self) {
        let _flag = BorrowFlagGuard::acquire();
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

/// Returns an unbound world reference; caller must exclude aliases and keep the context alive.
pub(super) unsafe fn get_world_mut<'a>() -> &'a mut WorldLocal {
    unsafe {
        current_world()
            .expect("flow world is not bound on this thread")
            .as_mut()
    }
}
