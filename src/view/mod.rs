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

/// A view over [`World`] that may be used to access specific components.
#[derive(Clone)]
#[must_use]
pub struct ViewValue<'a, Q: Query, F: Query, B: BorrowState> {
    archetypes: &'a [Archetype],
    query: Q,
    filter: F,
    state: B,
    entity_set: &'a EntitySet,
    epochs: &'a EpochCounter,
}

impl<'a, Q, F, B> Drop for ViewValue<'a, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline(always)]
    fn drop(&mut self) {
        self.release_borrow();
    }
}

impl<'a, Q, F, B> ViewValue<'a, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline(always)]
    fn acquire_borrow(&self) {
        self.state.acquire(self.query, self.filter, self.archetypes);
    }

    #[inline(always)]
    fn release_borrow(&self) {
        self.state.release(self.query, self.filter, self.archetypes);
    }

    #[inline(always)]
    fn with_borrow<R>(&self, arch_idx: u32, f: impl FnOnce() -> R) -> R {
        self.state.with(
            self.query,
            self.filter,
            &self.archetypes[arch_idx as usize],
            f,
        )
    }

    /// Releases borrow state and extracts it.
    #[inline(always)]
    fn extract_state(self) -> B {
        self.state.release(self.query, self.filter, self.archetypes);

        let me = core::mem::ManuallyDrop::new(self);
        // Safety: `state` will not be used after this due to `ManuallyDrop`.
        unsafe { core::ptr::read(&me.state) }
    }
}

impl<'a, Q, F, B> ViewValue<'a, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    /// Locks query borrows.
    ///
    /// Allocs to construct statically borrowed view with
    /// lifetime tied to this view borrow.
    ///
    /// This can improve performance if view is used multiple times where
    /// compiler can't figure it is borrowed already and perform check over and over.
    ///
    /// Additionally handy for existing API that requires `View`.
    ///
    /// For simmetry this method is avaialble for statically borrowed views too.
    #[inline(always)]
    pub fn lock(&mut self) -> ViewValue<'_, Q, F, StaticallyBorrowed> {
        self.acquire_borrow();
        ViewValue {
            archetypes: self.archetypes,
            query: self.query,
            filter: self.filter,
            state: StaticallyBorrowed,
            entity_set: self.entity_set,
            epochs: self.epochs,
        }
    }

    /// Unlocks query borrows.
    /// Allows usage of conflicting views.
    ///
    /// Borrows are automatically unlocked when the view is dropped.
    /// This method is necessary only if caller wants to keep the view
    /// to reuse it later.
    ///
    /// This method takes mutable reference to ensure that references
    /// created from this view are not used after it is unlocked.
    #[inline(always)]
    pub fn unlock(&mut self) {
        self.release_borrow()
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
            archetypes: world.archetypes(),
            query: query.into_query(),
            filter: filter.into_query(),
            state: StaticallyBorrowed,
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
            query: query.into_query(),
            filter: filter.into_query(),
            archetypes: world.archetypes(),
            state: StaticallyBorrowed,
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
            query: query.into_query(),
            filter: filter.into_query(),
            archetypes: world.archetypes(),
            state: RuntimeBorrowState::new(),
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
unsafe fn get_at<'a, Q, F>(
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
