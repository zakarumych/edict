use core::cell::Cell;

use crate::{
    archetype::{Archetype, chunk_idx},
    entity::Location,
    epoch::EpochSource,
    query::{Fetch, Query, QueryItem},
};

use super::{BorrowState, ViewValue};

pub(super) struct IndexedFetches<'a, Q: Query, F: Query> {
    query: Option<(u32, Q::Fetch<'a>)>,
    filter: Option<(u32, F::Fetch<'a>)>,
}

impl<Q: Query, F: Query> Default for IndexedFetches<'_, Q, F> {
    #[inline]
    fn default() -> Self {
        Self {
            query: None,
            filter: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CacheState {
    Idle,
    Busy,
    PendingClear,
}

/// Interior cache storage whose active entry stays at a stable address.
pub(super) struct FetchCache<T> {
    entry: Cell<Option<T>>,
    state: Cell<CacheState>,
}

impl<T> FetchCache<T> {
    #[inline]
    pub(super) const fn new() -> Self {
        Self {
            entry: Cell::new(None),
            state: Cell::new(CacheState::Idle),
        }
    }

    #[inline]
    pub(super) fn assert_idle(&self) {
        assert!(
            self.state.get() == CacheState::Idle,
            "Cannot release an active indexed fetch"
        );
    }

    #[inline]
    pub(super) fn clear(&self) {
        match self.state.get() {
            CacheState::Idle => {
                self.state.set(CacheState::Busy);
                self.clear_owned();
            }
            CacheState::Busy | CacheState::PendingClear => {
                self.state.set(CacheState::PendingClear);
            }
        }
    }

    #[inline]
    fn clear_owned(&self) {
        let _idle = ResetIdle(&self.state);
        // The owner ends all entry references before taking its storage.
        // Keeping the state busy also prevents Drop callbacks from entering it.
        drop(self.entry.take());
    }

    #[inline]
    fn enter(&self) -> Option<AccessGuard<'_, T>> {
        if self.state.get() != CacheState::Idle {
            return None;
        }
        // Reentrant accesses must see Busy before an entry reference is created.
        self.state.set(CacheState::Busy);
        Some(AccessGuard {
            cache: self,
            finished: false,
        })
    }

    #[cfg(all(test, feature = "std"))]
    fn is_empty(&self) -> bool {
        self.assert_idle();
        // Idle proves that no access guard holds a reference into the entry.
        unsafe { (&*self.entry.as_ptr()).is_none() }
    }
}

struct ResetIdle<'a>(&'a Cell<CacheState>);
impl Drop for ResetIdle<'_> {
    #[inline]
    fn drop(&mut self) {
        self.0.set(CacheState::Idle);
    }
}

struct AccessGuard<'a, T> {
    cache: &'a FetchCache<T>,
    finished: bool,
}

impl<T> AccessGuard<'_, T> {
    #[inline]
    fn entry(&mut self) -> &mut Option<T> {
        // Only this guard owns the Busy state. Reentrant code cannot access the
        // entry and clearing only records a pending request until this borrow ends.
        unsafe { &mut *self.cache.entry.as_ptr() }
    }

    #[inline]
    fn finish(mut self) {
        self.finished = true;
        let _idle = ResetIdle(&self.cache.state);
        if self.cache.state.get() == CacheState::PendingClear {
            drop(self.cache.entry.take());
        }
    }
}

impl<T> Drop for AccessGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        if !self.finished {
            // Short-lived fetch locals unwind before this guard. Always discard
            // retained state after a failed access and reset even if Drop panics.
            self.cache.clear_owned();
        }
    }
}

pub(super) struct ClearOnDrop<'a, T>(pub &'a FetchCache<T>);
impl<T> Drop for ClearOnDrop<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.0.clear();
    }
}

/// Indexes under the view's existing component access grant. Cached native fetches
/// retain raw storage handles; returned items are lent for the current view borrow.
#[inline]
pub(super) unsafe fn get_at_cached<'cache: 'get, 'get, Q, F, B, E, C>(
    view: &'get ViewValue<'cache, Q, F, B, E, C>,
    loc: Location,
) -> Option<QueryItem<'get, Q>>
where
    Q: Query,
    F: Query,
    B: BorrowState,
    C: EpochSource,
{
    let query = &view.query;
    let filter = &view.filter;
    let epochs = view.epochs;
    let last_fetches = &view.last_fetches;
    let archetype: &'cache Archetype = &view.archetypes[loc.arch as usize];
    let Some(mut access) = last_fetches.enter() else {
        // A shared reentrant access cannot touch the owner's active entry.
        // On failure, defer invalidation until the owner's fetches are dropped.
        let clear = ClearOnDrop(last_fetches);
        let result = unsafe { super::get_at(view.query, view.filter, epochs, archetype, loc) };
        core::mem::forget(clear);
        return result;
    };
    let result = {
        let cached = access.entry().get_or_insert_with(IndexedFetches::default);
        (|| {
            let Location { arch, idx } = loc;
            assert!(idx < archetype.len(), "Wrong location");
            if archetype.is_empty() {
                return None;
            }
            if !filter.visit_archetype(archetype, arch) || !query.visit_archetype(archetype, arch) {
                return None;
            }
            if !unsafe { filter.visit_archetype_late(archetype) }
                || !unsafe { query.visit_archetype_late(archetype) }
            {
                return None;
            }
            let epoch = epochs.next_if(Q::MUTABLE || F::MUTABLE);

            let mut filter_fetch = if F::SUPPORTS_FETCH_REUSE {
                if cached
                    .filter
                    .as_ref()
                    .is_some_and(|(last, _)| *last != arch)
                {
                    cached.filter = None;
                }
                if cached.filter.is_none() {
                    cached.filter = Some((arch, unsafe { filter.fetch(arch, archetype, epoch) }));
                }
                // Safety: only opted-in raw-handle fetches are retained, for this exact
                // live archetype and unchanged query. reuse_fetch lends a fresh handle.
                unsafe {
                    filter.reuse_fetch(
                        &mut cached.filter.as_mut().unwrap().1,
                        arch,
                        archetype,
                        epoch,
                    )
                }
            } else {
                unsafe { filter.fetch(arch, archetype, epoch) }
            };
            if !unsafe { filter_fetch.visit_chunk(chunk_idx(idx)) } {
                return None;
            }
            unsafe { filter_fetch.touch_chunk(chunk_idx(idx)) };
            if !unsafe { filter_fetch.visit_item(idx) } {
                return None;
            }

            let mut query_fetch = if Q::SUPPORTS_FETCH_REUSE {
                if cached.query.as_ref().is_some_and(|(last, _)| *last != arch) {
                    cached.query = None;
                }
                if cached.query.is_none() {
                    cached.query = Some((arch, unsafe { query.fetch(arch, archetype, epoch) }));
                }
                // Safety: same native-handle and archetype invariant as the filter.
                unsafe {
                    query.reuse_fetch(
                        &mut cached.query.as_mut().unwrap().1,
                        arch,
                        archetype,
                        epoch,
                    )
                }
            } else {
                unsafe { query.fetch(arch, archetype, epoch) }
            };
            if !unsafe { query_fetch.visit_chunk(chunk_idx(idx)) } {
                return None;
            }
            unsafe { query_fetch.touch_chunk(chunk_idx(idx)) };
            if !unsafe { query_fetch.visit_item(idx) } {
                return None;
            }
            Some(unsafe { query_fetch.get_item(idx) })
        })()
    };
    // Closure locals, including both short-lived fetches, are dropped before
    // pending invalidation can remove their retained source handles.
    access.finish();
    result
}

#[cfg(all(test, feature = "std"))]
mod tests;
