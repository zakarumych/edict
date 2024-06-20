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
/// Before this inevitable event happens, update to new version of this library with 128 bit counter will be required.
pub struct EpochCounter {
    value: AtomicU64,
}

impl EpochCounter {
    /// Returns new epoch counter.
    pub const fn new() -> Self {
        EpochCounter {
            value: AtomicU64::new(0),
        }
    }

    /// Returns current epoch id.
    #[inline(always)]
    pub fn current(&self) -> EpochId {
        EpochId {
            value: self.value.load(Ordering::Relaxed),
        }
    }

    /// Returns current epoch id.
    /// But faster.
    #[inline(always)]
    pub fn current_mut(&mut self) -> EpochId {
        EpochId {
            value: *self.value.get_mut(),
        }
    }

    /// Bumps to the next epoch and returns new epoch id.
    #[inline(always)]
    pub fn next(&self) -> EpochId {
        let old = self.value.fetch_add(1, Ordering::Relaxed);
        debug_assert!(old < u64::MAX);
        EpochId { value: old + 1 }
    }

    /// Bumps to the next epoch and returns new epoch id.
    /// But faster
    #[inline(always)]
    pub fn next_mut(&mut self) -> EpochId {
        let value = self.value.get_mut();
        debug_assert!(*value < u64::MAX);
        *value += 1;
        EpochId { value: *value }
    }

    /// Bumps to the next epoch and returns new epoch id if `cond` is true.
    /// Otherwise returns current epoch id.
    #[inline(always)]
    pub fn next_if(&self, cond: bool) -> EpochId {
        if cond {
            self.next()
        } else {
            self.current()
        }
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
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> core::fmt::Result {
        <u64 as Debug>::fmt(&self.value, f)
    }
}

impl EpochId {
    /// Returns id of starting epoch.
    #[inline(always)]
    pub const fn start() -> Self {
        EpochId { value: 0 }
    }

    /// Returns true if this epoch comes strictly before the `other`.
    #[inline(always)]
    pub const fn before(&self, other: EpochId) -> bool {
        self.value < other.value
    }

    /// Returns true if this epoch comes strictly after the `other`.
    #[inline(always)]
    pub const fn after(&self, other: EpochId) -> bool {
        self.value > other.value
    }

    /// Updates epoch id to later of this and the `other`.
    #[inline(always)]
    pub fn update(&mut self, other: EpochId) {
        self.value = self.value.max(other.value);
    }

    /// Bumps epoch to specified one.
    /// Assumes this epoch is strictly before epoch `to`.
    #[inline(always)]
    pub fn bump(&mut self, to: EpochId) {
        debug_assert!(
            self.before(to),
            "`EpochId::bump` must be used only for older epochs"
        );
        *self = to;
    }

    /// Bumps epoch to specified one.
    /// Assumes this epoch is before epoch `to` or the same.
    #[inline(always)]
    pub fn bump_again(&mut self, to: EpochId) {
        debug_assert!(
            !self.after(to),
            "`EpochId::bump` must be used only for older epochs"
        );
        *self = to;
    }

    /// Bumps epoch to specified one.
    /// Assumes this epoch is strictly before epoch to.
    #[inline(always)]
    pub fn bump_cell(cell: &Cell<Self>, to: EpochId) {
        debug_assert!(
            !cell.get().after(to),
            "`EpochId::bump_cell` must be used only for older or same epochs"
        );
        cell.set(to);
    }
}
