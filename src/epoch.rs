//! This module contains `EpochCounter` and `EpochId` types used for change detection.

use core::{
    cell::Cell,
    fmt::{self, Debug},
    sync::atomic::{AtomicU64, Ordering},
};

/// Monotonically incremented epoch counter.
/// It is assumed that underlying value cannot overflow in any reasonable amount of time.
/// For this purpose only increment operation is possible and counter starts with 0.
/// If incremented every nanosecond the counter will overflow in 14'029 years.
/// Before this inevitable event happens, update to new version of this library with 128 bit counter is required.
/// Which shall be released before year 14000 to give at least 29 years for migration.
pub struct EpochCounter {
    value: AtomicU64,
}

impl Default for EpochCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl EpochCounter {
    /// Returns new epoch counter.
    pub const fn new() -> Self {
        EpochCounter {
            value: AtomicU64::new(0),
        }
    }

    /// Returns current epoch id.
    #[inline]
    pub fn current(&self) -> EpochId {
        EpochId {
            value: self.value.load(Ordering::Relaxed),
        }
    }

    /// Returns current epoch id.
    /// But faster.
    #[inline]
    pub fn current_mut(&mut self) -> EpochId {
        EpochId {
            value: *self.value.get_mut(),
        }
    }

    /// Bumps to the next epoch and returns new epoch id.
    #[inline]
    pub fn next(&self) -> EpochId {
        let old = self.value.fetch_add(1, Ordering::Relaxed);
        debug_assert!(old < u64::MAX);
        EpochId { value: old + 1 }
    }

    /// Bumps to the next epoch and returns new epoch id.
    /// But faster
    #[inline]
    pub fn next_mut(&mut self) -> EpochId {
        let value = self.value.get_mut();
        debug_assert!(*value < u64::MAX);
        *value += 1;
        EpochId { value: *value }
    }

    /// Bumps to the next epoch and returns new epoch id if `cond` is true.
    /// Otherwise returns current epoch id.
    #[inline]
    pub fn next_if(&self, cond: bool) -> EpochId {
        if cond { self.next() } else { self.current() }
    }
}

/// Epoch identifier.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct EpochId {
    value: u64,
}

impl Default for EpochId {
    fn default() -> Self {
        EpochId::start()
    }
}

impl Debug for EpochId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> core::fmt::Result {
        <u64 as Debug>::fmt(&self.value, f)
    }
}

impl EpochId {
    /// Returns id of starting epoch.
    #[inline]
    pub const fn start() -> Self {
        EpochId { value: 0 }
    }

    /// Returns true if this epoch comes strictly before the `other`.
    #[inline]
    pub const fn before(&self, other: EpochId) -> bool {
        self.value < other.value
    }

    /// Returns true if this epoch comes strictly after the `other`.
    #[inline]
    pub const fn after(&self, other: EpochId) -> bool {
        self.value > other.value
    }

    /// Updates epoch id to later of this and the `other`.
    #[inline]
    pub fn update(&mut self, other: EpochId) {
        self.value = self.value.max(other.value);
    }

    /// Bumps epoch to specified one.
    /// Assumes this epoch is strictly before epoch `to`.
    #[inline]
    pub fn bump(&mut self, to: EpochId) {
        debug_assert!(
            self.before(to),
            "`EpochId::bump` must be used only for older epochs"
        );
        *self = to;
    }

    /// Bumps epoch to specified one.
    /// Assumes this epoch is before epoch `to` or the same.
    #[inline]
    pub fn bump_again(&mut self, to: EpochId) {
        debug_assert!(
            !self.after(to),
            "`EpochId::bump` must be used only for older epochs"
        );
        *self = to;
    }

    /// Bumps epoch to specified one.
    /// Assumes this epoch is strictly before epoch to.
    #[inline]
    pub fn bump_cell(cell: &Cell<Self>, to: EpochId) {
        debug_assert!(
            !cell.get().after(to),
            "`EpochId::bump_cell` must be used only for older or same epochs"
        );
        cell.set(to);
    }
}

mod sealed {
    pub trait Sealed {}
}

/// Epoch allocation used by views. Implementations are sealed.
pub trait EpochSource: sealed::Sealed + 'static {
    /// Returns the current epoch without changing it.
    fn current(&self) -> EpochId;
    /// Allocates a fresh epoch.
    fn next(&self) -> EpochId;
    /// Allocates a fresh epoch only for mutable access.
    #[inline]
    fn next_if(&self, mutable: bool) -> EpochId {
        if mutable { self.next() } else { self.current() }
    }
}

impl sealed::Sealed for EpochCounter {}
impl EpochSource for EpochCounter {
    #[inline]
    fn current(&self) -> EpochId {
        EpochCounter::current(self)
    }
    #[inline]
    fn next(&self) -> EpochId {
        EpochCounter::next(self)
    }
}

/// Epoch storage accessible only through an exclusive local-world capability.
/// Its `Cell` makes borrowed local views and iterators neither `Send` nor `Sync`.
///
/// ```compile_fail
/// use edict::view::LocalViewRef;
/// fn send<T: Send>() {}
/// send::<LocalViewRef<'static, ()>>();
/// ```
///
/// ```compile_fail
/// use edict::view::LocalViewOne;
/// fn sync<T: Sync>() {}
/// sync::<LocalViewOne<'static, ()>>();
/// ```
///
/// ```compile_fail
/// use edict::{epoch::LocalEpochCounter, view::ViewIter};
/// fn send<T: Send>() {}
/// send::<ViewIter<'static, (), (), LocalEpochCounter>>();
/// ```
///
/// ```compile_fail
/// use edict::{epoch::LocalEpochCounter, view::ViewBatchIter};
/// fn sync<T: Sync>() {}
/// sync::<ViewBatchIter<'static, (), (), LocalEpochCounter>>();
/// ```
#[repr(transparent)]
pub struct LocalEpochCounter {
    value: Cell<u64>,
}

impl LocalEpochCounter {
    /// Borrows the atomic's storage in exclusive local mode.
    ///
    /// # Safety
    /// No atomic access to this storage may overlap the returned reference or
    /// any view or iterator derived from it. The caller must retain an exclusive
    /// world capability that prevents shared-world access for that lifetime.
    #[inline]
    pub(crate) unsafe fn from_atomic(counter: &EpochCounter) -> &Self {
        // AtomicU64::as_ptr permits non-atomic access when synchronization rules
        // exclude concurrent atomic access. Cell and this wrapper are transparent
        // over u64, and AtomicU64 storage has sufficient alignment.
        unsafe { &*counter.value.as_ptr().cast::<Self>() }
    }
}

impl sealed::Sealed for LocalEpochCounter {}
impl EpochSource for LocalEpochCounter {
    #[inline]
    fn current(&self) -> EpochId {
        EpochId {
            value: self.value.get(),
        }
    }
    #[inline]
    fn next(&self) -> EpochId {
        let old = self.value.get();
        debug_assert!(old < u64::MAX);
        let value = old + 1;
        self.value.set(value);
        EpochId { value }
    }
}

#[cfg(test)]
mod local_epoch_tests {
    use super::*;

    #[test]
    fn local_source_updates_atomic_storage_with_fresh_epochs() {
        let counter = EpochCounter::new();
        let first = counter.next();
        let end;
        {
            // This test holds all access to the counter; no atomic operation is
            // performed while the local reference is in use.
            let source = unsafe { LocalEpochCounter::from_atomic(&counter) };
            assert_eq!(source.current(), first);
            assert_eq!(source.next_if(false), first);
            let second = source.next();
            assert!(second.after(first));
            end = source.next_if(true);
            assert!(end.after(second));
        }
        assert_eq!(counter.current(), end);
        assert!(counter.next().after(end));
    }
}
