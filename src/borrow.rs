//! Provides borrowing mechanism that is used by queries.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::query::Access;

// pub trait BorrowLock {
//     /// Locks for immutably borrow.
//     /// Returns true if successfully locked.
//     fn borrow(&self) -> bool;

//     /// Unlocks the immutable borrow.
//     ///
//     /// # Safety
//     ///
//     /// Must be called after successful call to `borrow`.
//     unsafe fn release(&self);

//     /// Locks for mutable borrow.
//     /// Returns true if successfully locked.
//     fn borrow_mut(&self) -> bool;

//     /// Unlocks the mutable borrow.
//     ///
//     /// # Safety
//     ///
//     /// Must be called after successful call to `borrow_mut`.
//     unsafe fn release_mut(&self);

//     /// Borrow using access value.
//     fn borrow_access(&self, access: Access) -> bool {
//         match access {
//             Access::Read => self.borrow(),
//             Access::Write => self.borrow_mut(),
//         }
//     }

//     /// Borrow using access value.
//     fn release_access(&self, access: Access) {
//         match access {
//             Access::Read => self.release(),
//             Access::Write => self.release_mut(),
//         }
//     }
// }

/// Thread-safe borrow lock that uses atomic operations.
#[repr(transparent)]
pub struct AtomicBorrowLock {
    state: AtomicUsize,
}

impl AtomicBorrowLock {
    pub const fn new() -> Self {
        Self {
            state: AtomicUsize::new(0),
        }
    }
}

const ATOMIC_MAX_BORROW: usize = usize::MAX >> 2;
const ATOMIC_MUTABLE_LOCK: usize = 1 + (usize::MAX >> 1);

impl AtomicBorrowLock {
    #[inline]
    pub fn borrow(&self) -> bool {
        let old = self.state.fetch_add(1, Ordering::Acquire);

        if old < ATOMIC_MUTABLE_LOCK {
            if old >= ATOMIC_MAX_BORROW {
                self.state.fetch_sub(1, Ordering::Release);
                too_many_refs();
            }
            true
        } else {
            self.state.fetch_sub(1, Ordering::Release);
            false
        }
    }

    #[inline]
    pub unsafe fn release(&self) {
        let old = self.state.fetch_sub(1, Ordering::Release);
        debug_assert!(old < ATOMIC_MUTABLE_LOCK);
    }

    #[inline]
    pub fn borrow_mut(&self) -> bool {
        let res = self.state.compare_exchange(
            0,
            ATOMIC_MUTABLE_LOCK,
            Ordering::Acquire,
            Ordering::Relaxed,
        );

        return res.is_ok();
    }

    #[inline]
    pub unsafe fn release_mut(&self) {
        let old = self.state.fetch_sub(ATOMIC_MUTABLE_LOCK, Ordering::Release);
        debug_assert!(old >= ATOMIC_MUTABLE_LOCK);
    }

    /// Borrow using access value.
    #[inline]
    pub fn borrow_access(&self, access: Access) -> bool {
        match access {
            Access::Read => self.borrow(),
            Access::Write => self.borrow_mut(),
        }
    }

    /// Borrow using access value.
    #[inline]
    pub unsafe fn release_access(&self, access: Access) {
        match access {
            Access::Read => self.release(),
            Access::Write => self.release_mut(),
        }
    }
}

// /// Non-thread-safe borrow lock that uses [`Cell`].
// #[repr(transparent)]
// pub struct LocalBorrowLock {
//     state: Cell<usize>,
// }

// impl LocalBorrowLock {
//     pub const fn new() -> Self {
//         Self {
//             state: Cell::new(0),
//         }
//     }
// }

// const LOCAL_MAX_BORROW: usize = usize::MAX - 1;
// const LOCAL_MUTABLE_LOCK: usize = usize::MAX;

// impl LocalBorrowLock {
//     #[inline]
//     pub fn borrow(&self) -> bool {
//         let old = self.state.get();

//         if old < LOCAL_MUTABLE_LOCK {
//             if old >= LOCAL_MAX_BORROW {
//                 too_many_refs();
//             }
//             self.state.set(old + 1);
//             true
//         } else {
//             false
//         }
//     }

//     #[inline]
//     pub unsafe fn release(&self) {
//         debug_assert_ne!(self.state.get(), 0);
//         debug_assert_ne!(self.state.get(), usize::MAX);
//         self.state.set(self.state.get() - 1);
//     }

//     #[inline]
//     pub fn borrow_mut(&self) -> bool {
//         let old = self.state.get();

//         if old == 0 {
//             self.state.set(usize::MAX);
//             true
//         } else {
//             false
//         }
//     }

//     #[inline]
//     pub unsafe fn release_mut(&self) {
//         debug_assert_eq!(self.state.get(), usize::MAX);
//         self.state.set(0);
//     }

//     /// Borrow using access value.
//     #[inline]
//     pub fn borrow_access(&self, access: Access) -> bool {
//         match access {
//             Access::Read => self.borrow(),
//             Access::Write => self.borrow_mut(),
//         }
//     }

//     /// Borrow using access value.
//     #[inline]
//     pub unsafe fn release_access(&self, access: Access) {
//         match access {
//             Access::Read => self.release(),
//             Access::Write => self.release_mut(),
//         }
//     }
// }

#[inline(never)]
#[track_caller]
#[cold]
const fn too_many_refs() -> ! {
    panic!("Too many borrows");
}
