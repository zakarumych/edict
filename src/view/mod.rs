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
    borrow::{BorrowState, RuntimeBorrowState, StaticallyBorrowed},
    one::{ViewOne, ViewOneState},
};

mod borrow;
mod extend;
mod index;
mod iter;
mod one;

/// A view over [`World`] that may be used to access specific components.
#[must_use]
pub struct ViewValue<'a, Q: Query, F: Query, B> {
    query: Q,
    filter: F,
    archetypes: &'a [Archetype],
    entity_set: &'a EntitySet,
    borrow: B,
    epochs: &'a EpochCounter,
}

/// View over entities that match query and filter, restricted to
/// components that match the query.
pub type View<'a, Q, F = (), B = RuntimeBorrowState> =
    ViewValue<'a, <Q as IntoQuery>::Query, <F as IntoQuery>::Query, B>;

/// View over entities that match query and filter, restricted to
/// components that match the query.
pub type ViewMut<'a, Q, F = ()> =
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
    pub fn new_mut(world: &'a mut World, query: Q, filter: F) -> Self {
        ViewValue {
            query: query.into_query(),
            filter: filter.into_query(),
            archetypes: world.archetypes(),
            entity_set: world.entities(),
            borrow: StaticallyBorrowed,
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
            entity_set: world.entities(),
            borrow: StaticallyBorrowed,
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
    pub fn new(world: &'a World, query: Q, filter: F) -> Self {
        ViewValue {
            query: query.into_query(),
            filter: filter.into_query(),
            archetypes: world.archetypes(),
            entity_set: world.entities(),
            borrow: RuntimeBorrowState::new(),
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
    query: &Q,
    filter: &F,
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

    if !unsafe { Query::visit_archetype(query, archetype) } {
        return None;
    }

    if !unsafe { Query::visit_archetype(filter, archetype) } {
        return None;
    }

    let epoch = epochs.next_if(Q::Query::MUTABLE || F::Query::MUTABLE);

    let mut query_fetch = unsafe { Query::fetch(query, arch, archetype, epoch) };

    if !unsafe { Fetch::visit_chunk(&mut query_fetch, chunk_idx(idx)) } {
        return None;
    }

    unsafe { Fetch::touch_chunk(&mut query_fetch, chunk_idx(idx)) }

    if !unsafe { Fetch::visit_item(&mut query_fetch, idx) } {
        return None;
    }

    let mut filter_fetch = unsafe { Query::fetch(filter, arch, archetype, epoch) };

    if !unsafe { Fetch::visit_chunk(&mut filter_fetch, chunk_idx(idx)) } {
        return None;
    }

    unsafe { Fetch::touch_chunk(&mut filter_fetch, chunk_idx(idx)) }

    if !unsafe { Fetch::visit_item(&mut filter_fetch, idx) } {
        return None;
    }

    Some(unsafe { Fetch::get_item(&mut query_fetch, idx) })
}
