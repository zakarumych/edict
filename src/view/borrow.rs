use core::cell::Cell;

use crate::{
    archetype::Archetype,
    query::{Query, QueryCacheRef},
};

/// A view borrow state.
///
/// View must borrow components it accesses before
/// dereferencing any pointers.
pub trait BorrowState {
    /// Borrow components in the archetype if not already borrowed.
    fn acquire<Q: Query, F: Query>(
        &self,
        query: QueryCacheRef<'_, Q>,
        filter: QueryCacheRef<'_, F>,
        archetypes: &[Archetype],
    );

    /// Release previously acquired borrow.
    fn release<Q: Query, F: Query>(
        &self,
        query: QueryCacheRef<'_, Q>,
        filter: QueryCacheRef<'_, F>,
        archetypes: &[Archetype],
    );

    /// Temporarily acquire borrow and call `f`.
    #[allow(clippy::too_many_arguments)]
    fn with<Q: Query, F: Query, R>(
        &self,
        query: QueryCacheRef<'_, Q>,
        filter: QueryCacheRef<'_, F>,
        archetype: &Archetype,
        idx: u32,
        f: impl FnOnce() -> R,
    ) -> R;
}

/// Acquire borrow on archetypes.
/// The slice must contain the world's complete archetype list in its original order.
#[inline]
#[track_caller]
pub fn acquire<Q: Query, F: Query>(
    query: QueryCacheRef<'_, Q>,
    filter: QueryCacheRef<'_, F>,
    archetypes: &[Archetype],
) {
    struct ReleaseOnFailure<'a, Q: Query, F: Query> {
        archetypes: &'a [Archetype],
        query: QueryCacheRef<'a, Q>,
        filter: QueryCacheRef<'a, F>,
        query_len: usize,
        filter_len: usize,
    }

    impl<'a, Q, F> Drop for ReleaseOnFailure<'a, Q, F>
    where
        Q: Query,
        F: Query,
    {
        fn drop(&mut self) {
            for (idx, archetype) in self.archetypes.iter().enumerate() {
                unsafe {
                    if self.filter.visit_archetype(archetype, idx as u32)
                        && self.query.visit_archetype(archetype, idx as u32)
                    {
                        self.query.access_archetype(archetype, |id, access| {
                            if self.query_len > 0 {
                                if let Some(component) = archetype.component(id) {
                                    component.release(access);
                                }
                                self.query_len -= 1;
                            }
                        });

                        self.filter.access_archetype(archetype, |id, access| {
                            if self.filter_len > 0 {
                                if let Some(component) = archetype.component(id) {
                                    component.release(access);
                                }
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

    for (idx, archetype) in archetypes.iter().enumerate() {
        unsafe {
            if filter.visit_archetype(archetype, idx as u32)
                && query.visit_archetype(archetype, idx as u32)
            {
                query.access_archetype(archetype, |id, access| {
                    if let Some(component) = archetype.component(id) {
                        let success = component.borrow(access);
                        assert!(success, "Failed to lock '{id:?}' from archetype");
                    }
                    guard.query_len += 1;
                });
                filter.access_archetype(archetype, |id, access| {
                    if let Some(component) = archetype.component(id) {
                        let success = component.borrow(access);
                        assert!(success, "Failed to lock '{id:?}' from archetype");
                    }
                    guard.filter_len += 1;
                });
            }
        }
    }

    core::mem::forget(guard);
}

/// Release borrow on archetypes.
/// Pass the same complete archetype list and caches used to acquire the borrow.
#[inline]
pub fn release<Q: Query, F: Query>(
    query: QueryCacheRef<'_, Q>,
    filter: QueryCacheRef<'_, F>,
    archetypes: &[Archetype],
) {
    for (idx, archetype) in archetypes.iter().enumerate() {
        unsafe {
            if filter.visit_archetype(archetype, idx as u32)
                && query.visit_archetype(archetype, idx as u32)
            {
                query.access_archetype(archetype, &|id, access| {
                    if let Some(component) = archetype.component(id) {
                        component.release(access);
                    }
                });
                filter.access_archetype(archetype, &|id, access| {
                    if let Some(component) = archetype.component(id) {
                        component.release(access);
                    }
                });
            }
        }
    }
}

#[inline]
#[track_caller]
fn acquire_one<Q: Query, F: Query>(
    query: QueryCacheRef<'_, Q>,
    filter: QueryCacheRef<'_, F>,
    archetype: &Archetype,
    idx: u32,
) {
    struct ReleaseOnFailure<'a, Q: Query, F: Query> {
        archetype: &'a Archetype,
        query: QueryCacheRef<'a, Q>,
        filter: QueryCacheRef<'a, F>,
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
                            if let Some(component) = self.archetype.component(id) {
                                component.release(access);
                            }
                            self.query_len -= 1;
                        }
                    });

                    self.filter.access_archetype(self.archetype, |id, access| {
                        if self.filter_len > 0 {
                            if let Some(component) = self.archetype.component(id) {
                                component.release(access);
                            }
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
        if filter.visit_archetype(archetype, idx) && query.visit_archetype(archetype, idx) {
            query.access_archetype(archetype, |id, access| {
                if let Some(component) = archetype.component(id) {
                    let success = component.borrow(access);
                    assert!(success, "Failed to lock '{id:?}' from archetype");
                }
                guard.query_len += 1;
            });
            filter.access_archetype(archetype, |id, access| {
                if let Some(component) = archetype.component(id) {
                    let success = component.borrow(access);
                    assert!(success, "Failed to lock '{id:?}' from archetype");
                }
                guard.filter_len += 1;
            });
        }
    }

    core::mem::forget(guard);
}

#[inline]
fn release_one<Q: Query, F: Query>(
    query: QueryCacheRef<'_, Q>,
    filter: QueryCacheRef<'_, F>,
    archetype: &Archetype,
    idx: u32,
) {
    unsafe {
        if filter.visit_archetype(archetype, idx) && query.visit_archetype(archetype, idx) {
            query.access_archetype(archetype, &|id, access| {
                if let Some(component) = archetype.component(id) {
                    component.release(access);
                }
            });
            filter.access_archetype(archetype, &|id, access| {
                if let Some(component) = archetype.component(id) {
                    component.release(access);
                }
            });
        }
    }
}

/// Borrow state for runtime borrowing.
///
/// [`ViewCell`](crate::view::ViewCell) is type alias for view with this borrow state.
///
/// Views with this state can be created from shared world reference,
/// but would require runtime borrow checks for components.
/// Mutably aliased borrows will result in runtime panic.
///
/// [`System`](crate::system::System)s may use this type of borrow state when aliasing views are required.
pub struct RuntimeBorrowState {
    borrowed: Cell<bool>,
}

impl Default for RuntimeBorrowState {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeBorrowState {
    /// Create a new [`RuntimeBorrowState`] in the un-borrowed state.
    pub const fn new() -> Self {
        RuntimeBorrowState {
            borrowed: Cell::new(false),
        }
    }

    pub(crate) fn acquire_one<Q: Query, F: Query>(
        &self,
        query: QueryCacheRef<'_, Q>,
        filter: QueryCacheRef<'_, F>,
        archetype: &Archetype,
        idx: u32,
    ) {
        if !self.borrowed.get() {
            acquire_one(query, filter, archetype, idx);
            self.borrowed.set(true);
        }
    }

    pub(crate) fn release_one<Q: Query, F: Query>(
        &self,
        query: QueryCacheRef<'_, Q>,
        filter: QueryCacheRef<'_, F>,
        archetype: &Archetype,
        idx: u32,
    ) {
        if self.borrowed.take() {
            release_one(query, filter, archetype, idx);
        }
    }
}

impl BorrowState for RuntimeBorrowState {
    #[inline]
    fn acquire<Q: Query, F: Query>(
        &self,
        query: QueryCacheRef<'_, Q>,
        filter: QueryCacheRef<'_, F>,
        archetypes: &[Archetype],
    ) {
        if !self.borrowed.get() {
            acquire(query, filter, archetypes);
            self.borrowed.set(true);
        }
    }

    #[inline]
    fn release<Q: Query, F: Query>(
        &self,
        query: QueryCacheRef<'_, Q>,
        filter: QueryCacheRef<'_, F>,
        archetypes: &[Archetype],
    ) {
        if !self.borrowed.take() {
            return;
        }

        release(query, filter, archetypes);
    }

    #[inline]
    fn with<Q: Query, F: Query, R>(
        &self,
        query: QueryCacheRef<'_, Q>,
        filter: QueryCacheRef<'_, F>,
        archetype: &Archetype,
        idx: u32,
        f: impl FnOnce() -> R,
    ) -> R {
        if !self.borrowed.get() {
            acquire_one(query, filter, archetype, idx);
        }
        let r = f();
        if !self.borrowed.get() {
            release_one(query, filter, archetype, idx);
        }
        r
    }
}

/// Borrow state for static borrowing.
///
/// [`View`](crate::view::View) and [`ViewMut`](crate::view::ViewMut) are type aliases for view with this borrow state.
///
/// Views with this state can be created unsafely from shared world reference.
/// User must ensure that no mutably aliased borrows are created.
///
/// For this reason, this type of borrow state does not allow extending views.
///
/// [`System`](crate::system::System)s commonly use this type of borrow state.
/// Scheduler plans systems execution to eliminate any possible mutably aliased borrows.
#[derive(Copy, Clone, Debug)]
pub struct StaticallyBorrowed;

impl BorrowState for StaticallyBorrowed {
    #[inline]
    fn acquire<Q: Query, F: Query>(
        &self,
        _query: QueryCacheRef<'_, Q>,
        _filter: QueryCacheRef<'_, F>,
        _archetypes: &[Archetype],
    ) {
    }

    #[inline]
    fn release<Q: Query, F: Query>(
        &self,
        _query: QueryCacheRef<'_, Q>,
        _filter: QueryCacheRef<'_, F>,
        _archetypes: &[Archetype],
    ) {
    }

    #[inline]
    fn with<Q: Query, F: Query, R>(
        &self,
        _query: QueryCacheRef<'_, Q>,
        _filter: QueryCacheRef<'_, F>,
        _archetype: &Archetype,
        _idx: u32,
        f: impl FnOnce() -> R,
    ) -> R {
        f()
    }
}

impl From<StaticallyBorrowed> for RuntimeBorrowState {
    #[inline]
    fn from(_: StaticallyBorrowed) -> Self {
        RuntimeBorrowState::new()
    }
}
