use core::cell::Cell;

use crate::{archetype::Archetype, query::Query};

/// A view borrow state.
///
/// View must borrow components it accesses before
/// dereferencing any pointers.
pub trait BorrowState {
    /// Borrow components in the archetype if not already borrowed.
    fn acquire<Q: Query, F: Query>(&self, query: Q, filter: F, archetypes: &[Archetype]);

    /// Release previously acquired borrow.
    fn release<Q: Query, F: Query>(&self, query: Q, filter: F, archetypes: &[Archetype]);

    /// Temporarily acquire borrow and call `f`.
    fn with<Q: Query, F: Query, R>(
        &self,
        query: Q,
        filter: F,
        archetype: &Archetype,
        f: impl FnOnce() -> R,
    ) -> R;
}

/// A view borrow state that allows view to be extended.
pub trait ExtendableBorrowState: BorrowState {}

/// Borrow state for runtime borrowing.
pub struct RuntimeBorrowState {
    borrowed: Cell<bool>,
}

impl RuntimeBorrowState {
    /// Create a new [`RuntimeBorrowState`] in the un-borrowed state.
    pub const fn new() -> Self {
        RuntimeBorrowState {
            borrowed: Cell::new(false),
        }
    }
}

/// Acquire borrow on archetypes.
#[inline(always)]
#[track_caller]
pub fn acquire<Q: Query, F: Query>(query: Q, filter: F, archetypes: &[Archetype]) {
    struct ReleaseOnFailure<'a, Q: Query, F: Query> {
        archetypes: &'a [Archetype],
        query: Q,
        filter: F,
        query_len: usize,
        filter_len: usize,
    }

    impl<'a, Q, F> Drop for ReleaseOnFailure<'a, Q, F>
    where
        Q: Query,
        F: Query,
    {
        fn drop(&mut self) {
            for archetype in self.archetypes {
                unsafe {
                    if self.query.visit_archetype(archetype)
                        && self.filter.visit_archetype(archetype)
                    {
                        self.query.access_archetype(archetype, |id, access| {
                            if self.query_len > 0 {
                                archetype.component(id).unwrap_unchecked().release(access);
                                self.query_len -= 1;
                            }
                        });

                        self.filter.access_archetype(archetype, |id, access| {
                            if self.filter_len > 0 {
                                archetype.component(id).unwrap_unchecked().release(access);
                                self.filter_len -= 1;
                            }
                        });
                    }
                }
            }
        }
    }

    let mut guard = ReleaseOnFailure {
        archetypes,
        query,
        filter,
        query_len: 0,
        filter_len: 0,
    };

    for archetype in archetypes {
        unsafe {
            if query.visit_archetype(archetype) && filter.visit_archetype(archetype) {
                query.access_archetype(archetype, |id, access| {
                    let success = archetype.component(id).unwrap_unchecked().borrow(access);
                    assert!(success, "Failed to lock '{:?}' from archetype", id);
                    guard.query_len += 1;
                });
                filter.access_archetype(archetype, |id, access| {
                    let success = archetype.component(id).unwrap_unchecked().borrow(access);
                    assert!(success, "Failed to lock '{:?}' from archetype", id);
                    guard.filter_len += 1;
                });
            }
        }
    }

    core::mem::forget(guard);
}

/// Release borrow on archetypes.
#[inline(always)]
pub fn release<Q: Query, F: Query>(query: Q, filter: F, archetypes: &[Archetype]) {
    for archetype in archetypes {
        unsafe {
            if query.visit_archetype(archetype) && filter.visit_archetype(archetype) {
                query.access_archetype(archetype, &|id, access| {
                    archetype.component(id).unwrap_unchecked().release(access);
                });
                filter.access_archetype(archetype, &|id, access| {
                    archetype.component(id).unwrap_unchecked().release(access);
                });
            }
        }
    }
}

#[inline(always)]
#[track_caller]
fn acquire_one<Q: Query, F: Query>(query: Q, filter: F, archetype: &Archetype) {
    struct ReleaseOnFailure<'a, Q: Query, F: Query> {
        archetype: &'a Archetype,
        query: Q,
        filter: F,
        query_len: usize,
        filter_len: usize,
    }

    impl<'a, Q, F> Drop for ReleaseOnFailure<'a, Q, F>
    where
        Q: Query,
        F: Query,
    {
        fn drop(&mut self) {
            if self.query_len > 0 || self.filter_len > 0 {
                unsafe {
                    self.query.access_archetype(self.archetype, |id, access| {
                        if self.query_len > 0 {
                            self.archetype
                                .component(id)
                                .unwrap_unchecked()
                                .release(access);
                            self.query_len -= 1;
                        }
                    });

                    self.filter.access_archetype(self.archetype, |id, access| {
                        if self.filter_len > 0 {
                            self.archetype
                                .component(id)
                                .unwrap_unchecked()
                                .release(access);
                            self.filter_len -= 1;
                        }
                    });
                }
            }
        }
    }

    let mut guard = ReleaseOnFailure {
        archetype,
        query,
        filter,
        query_len: 0,
        filter_len: 0,
    };

    unsafe {
        if query.visit_archetype(archetype) && filter.visit_archetype(archetype) {
            query.access_archetype(archetype, |id, access| {
                let success = archetype.component(id).unwrap_unchecked().borrow(access);
                assert!(success, "Failed to lock '{:?}' from archetype", id);
                guard.query_len += 1;
            });
            filter.access_archetype(archetype, |id, access| {
                let success = archetype.component(id).unwrap_unchecked().borrow(access);
                assert!(success, "Failed to lock '{:?}' from archetype", id);
                guard.filter_len += 1;
            });
        }
    }

    core::mem::forget(guard);
}

#[inline(always)]
fn release_one<Q: Query, F: Query>(query: Q, filter: F, archetype: &Archetype) {
    unsafe {
        if query.visit_archetype(archetype) && filter.visit_archetype(archetype) {
            query.access_archetype(archetype, &|id, access| {
                archetype.component(id).unwrap_unchecked().release(access);
            });
            filter.access_archetype(archetype, &|id, access| {
                archetype.component(id).unwrap_unchecked().release(access);
            });
        }
    }
}

impl BorrowState for RuntimeBorrowState {
    #[inline(always)]
    fn acquire<Q: Query, F: Query>(&self, query: Q, filter: F, archetypes: &[Archetype]) {
        if !self.borrowed.get() {
            acquire(query, filter, archetypes);
            self.borrowed.set(true);
        }
    }

    #[inline(always)]
    fn release<Q: Query, F: Query>(&self, query: Q, filter: F, archetypes: &[Archetype]) {
        if !self.borrowed.take() {
            return;
        }

        release(query, filter, archetypes);
    }

    #[inline(always)]
    fn with<Q: Query, F: Query, R>(
        &self,
        query: Q,
        filter: F,
        archetype: &Archetype,
        f: impl FnOnce() -> R,
    ) -> R {
        if !self.borrowed.get() {
            acquire_one(query, filter, archetype);
        }
        let r = f();
        if !self.borrowed.get() {
            release_one(query, filter, archetype);
        }
        r
    }
}

impl ExtendableBorrowState for RuntimeBorrowState {}

/// Borrow state for statically borrowed views.
/// These can be created from [`&mut World`](crate::world::World)
/// or unsafely from [`&World`](crate::world::World).
#[derive(Copy, Clone, Debug)]
pub struct StaticallyBorrowed;

impl BorrowState for StaticallyBorrowed {
    #[inline(always)]
    fn acquire<Q: Query, F: Query>(&self, _query: Q, _filter: F, _archetypes: &[Archetype]) {}

    #[inline(always)]
    fn release<Q: Query, F: Query>(&self, _query: Q, _filter: F, _archetypes: &[Archetype]) {}

    #[inline(always)]
    fn with<Q: Query, F: Query, R>(
        &self,
        _query: Q,
        _filter: F,
        _archetype: &Archetype,
        f: impl FnOnce() -> R,
    ) -> R {
        f()
    }
}

/// Borrow state for statically borrowed views.
/// These can be created from [`&mut World`](crate::world::World)
/// or unsafely from [`&World`](crate::world::World).
#[derive(Copy, Clone, Debug)]
pub struct ExclusivelyBorrowed;

impl BorrowState for ExclusivelyBorrowed {
    #[inline(always)]
    fn acquire<Q: Query, F: Query>(&self, _query: Q, _filter: F, _archetypes: &[Archetype]) {}

    #[inline(always)]
    fn release<Q: Query, F: Query>(&self, _query: Q, _filter: F, _archetypes: &[Archetype]) {}

    #[inline(always)]
    fn with<Q: Query, F: Query, R>(
        &self,
        _query: Q,
        _filter: F,
        _archetype: &Archetype,
        f: impl FnOnce() -> R,
    ) -> R {
        f()
    }
}

impl ExtendableBorrowState for ExclusivelyBorrowed {}
