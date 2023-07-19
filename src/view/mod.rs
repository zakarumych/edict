//! A view over [`World`] that may be used to access specific components.
//!
//! The world can be seen as a table. Then entities would be rows and components would be columns.
//! Then [`View`] is a columnar slice of the table with filtering.

use crate::{
    archetype::{chunk_idx, Archetype},
    entity::{EntitySet, Location},
    epoch::EpochCounter,
    query::{Fetch, IntoQuery, Query, QueryItem},
    world::World,
};

pub use self::{
    borrow::{acquire, release, BorrowState, RuntimeBorrowState, StaticallyBorrowed},
    iter::ViewIter,
    one::{ViewOne, ViewOneState},
};

mod borrow;
mod extend;
mod index;
mod iter;
mod one;

#[derive(Clone)]
struct ViewBorrow<'a, Q: Query, F: Query, B: BorrowState> {
    archetypes: &'a [Archetype],
    query: Q,
    filter: F,
    state: B,
}

impl<'a, Q, F, B> Drop for ViewBorrow<'a, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline(always)]
    fn drop(&mut self) {
        self.state
            .release(&self.query, &self.filter, self.archetypes);
    }
}

impl<'a, Q, F, B> ViewBorrow<'a, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline(always)]
    fn acquire(&self) {
        self.state
            .acquire(&self.query, &self.filter, self.archetypes);
    }

    #[inline(always)]
    fn release(&self) {
        self.state
            .release(&self.query, &self.filter, self.archetypes);
    }

    #[inline(always)]
    fn with<R>(&self, arch_idx: u32, f: impl FnOnce() -> R) -> R {
        self.state.with(
            &self.query,
            &self.filter,
            &self.archetypes[arch_idx as usize],
            f,
        )
    }

    #[inline(always)]
    fn split(self) -> (&'a [Archetype], Q, F, B) {
        self.state
            .release(&self.query, &self.filter, self.archetypes);

        unsafe {
            let me = std::mem::ManuallyDrop::new(self);
            let archetypes = std::ptr::read(&me.archetypes);
            let query = std::ptr::read(&me.query);
            let filter = std::ptr::read(&me.filter);
            let state = std::ptr::read(&me.state);
            (archetypes, query, filter, state)
        }
    }
}

/// A view over [`World`] that may be used to access specific components.
#[derive(Clone)]
#[must_use]
pub struct ViewValue<'a, Q: Query, F: Query, B: BorrowState> {
    entity_set: &'a EntitySet,
    epochs: &'a EpochCounter,
    borrow: ViewBorrow<'a, Q, F, B>,
}

impl<'a, Q, F> ViewValue<'a, Q, F, RuntimeBorrowState>
where
    Q: Query,
    F: Query,
{
    /// Unlocks runtime borrows.
    /// Allows usage of conflicting views.
    ///
    /// Borrows are automatically unlocked when the view is dropped.
    /// This method is necessary only if caller wants to keep the view
    /// to reuse it later.
    pub fn unlock(&self) {
        self.borrow.release()
    }
}

/// View over entities that match query and filter, restricted to
/// components that match the query.
pub type ViewCell<'a, Q, F = (), B = RuntimeBorrowState> =
    ViewValue<'a, <Q as IntoQuery>::Query, <F as IntoQuery>::Query, B>;

/// View over entities that match query and filter, restricted to
/// components that match the query.
pub type View<'a, Q, F = ()> =
    ViewValue<'a, <Q as IntoQuery>::Query, <F as IntoQuery>::Query, StaticallyBorrowed>;

impl<'a, Q, F> ViewValue<'a, Q, F, StaticallyBorrowed>
where
    Q: Query,
    F: Query,
{
    /// Creates a new view over the world.
    /// Borrows the world mutably, so no other views can be created.
    /// In exchange it does not require runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline(always)]
    pub fn new(world: &'a mut World, query: Q, filter: F) -> Self {
        ViewValue {
            borrow: ViewBorrow {
                archetypes: world.archetypes(),
                query: query.into_query(),
                filter: filter.into_query(),
                state: StaticallyBorrowed,
            },
            entity_set: world.entities(),
            epochs: world.epoch_counter(),
        }
    }

    /// Creates a new view over the world.
    /// This is unsafe because it does not perform runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline(always)]
    pub unsafe fn new_unchecked(world: &'a World, query: Q, filter: F) -> Self {
        ViewValue {
            borrow: ViewBorrow {
                query: query.into_query(),
                filter: filter.into_query(),
                archetypes: world.archetypes(),
                state: StaticallyBorrowed,
            },
            entity_set: world.entities(),
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> ViewValue<'a, Q, F, RuntimeBorrowState>
where
    Q: Query,
    F: Query,
{
    /// Creates a new view over the world.
    /// Performs runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline(always)]
    pub fn new_cell(world: &'a World, query: Q, filter: F) -> Self {
        ViewValue {
            borrow: ViewBorrow {
                query: query.into_query(),
                filter: filter.into_query(),
                archetypes: world.archetypes(),
                state: RuntimeBorrowState::new(),
            },
            entity_set: world.entities(),
            epochs: world.epoch_counter(),
        }
    }
}

#[inline(always)]
#[track_caller]
fn expect_match<T>(value: Option<T>) -> T {
    value.expect("Entity does not match view's query and filter")
}

#[inline(always)]
#[track_caller]
fn expect_alive<T>(value: Option<T>) -> T {
    value.expect("Entity is not alive")
}

#[inline(always)]
fn get_at<'a, Q, F>(
    query: Q,
    filter: F,
    epochs: &EpochCounter,
    archetype: &'a Archetype,
    loc: Location,
) -> Option<QueryItem<'a, Q>>
where
    Q: Query,
    F: Query,
{
    let Location { arch, idx } = loc;
    assert!(idx < archetype.len() as u32, "Wrong location");

    if !unsafe { query.visit_archetype(archetype) } {
        return None;
    }

    if !unsafe { filter.visit_archetype(archetype) } {
        return None;
    }

    let epoch = epochs.next_if(Q::Query::MUTABLE || F::Query::MUTABLE);

    let mut query_fetch = unsafe { query.fetch(arch, archetype, epoch) };

    if !unsafe { Fetch::visit_chunk(&mut query_fetch, chunk_idx(idx)) } {
        return None;
    }

    unsafe { Fetch::touch_chunk(&mut query_fetch, chunk_idx(idx)) }

    if !unsafe { Fetch::visit_item(&mut query_fetch, idx) } {
        return None;
    }

    let mut filter_fetch = unsafe { filter.fetch(arch, archetype, epoch) };

    if !unsafe { Fetch::visit_chunk(&mut filter_fetch, chunk_idx(idx)) } {
        return None;
    }

    unsafe { Fetch::touch_chunk(&mut filter_fetch, chunk_idx(idx)) }

    if !unsafe { Fetch::visit_item(&mut filter_fetch, idx) } {
        return None;
    }

    Some(unsafe { Fetch::get_item(&mut query_fetch, idx) })
}
