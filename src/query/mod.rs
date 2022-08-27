//! Queries and iterators.
//!
//! To efficiently iterate over entities with specific set of components,
//! or only over those where specific component is modified, or missing,
//! [`Query`] is the solution.
//!
//! [`Query`] trait has a lot of implementations and is composable using tuples.

use core::any::TypeId;

use crate::{archetype::Archetype, epoch::EpochId};

pub use self::{
    alt::{Alt, FetchAlt},
    borrow::{
        FetchBorrowAllRead, FetchBorrowAnyRead, FetchBorrowAnyWrite, FetchBorrowOneRead,
        FetchBorrowOneWrite, QueryBorrowAll, QueryBorrowAny, QueryBorrowOne,
    },
    fetch::{Fetch, VerifyFetch},
    filter::{Filter, FilteredFetch, FilteredQuery, IntoFilter, With, Without},
    iter::QueryIter,
    modified::{Modified, ModifiedFetchAlt, ModifiedFetchRead, ModifiedFetchWrite},
    phantom::{ImmutablePhantomQuery, PhantomQuery, PhantomQueryFetch},
    read::{read, FetchRead},
    write::{write, FetchWrite},
};

mod alt;
mod borrow;
mod fetch;
mod filter;
mod iter;
mod modified;
mod option;
mod phantom;
mod read;
mod tuple;
mod write;

/// Specifies kind of access query performs for particular component.
#[derive(Clone, Copy, Debug)]
pub enum Access {
    /// Shared access to component. Can be aliased with other [`Access::Read`] accesses.
    Read,

    /// Cannot be aliased with any other access.
    Write,
}

/// Types associated with a query type.
pub trait IntoQuery {
    /// Associated query type.
    type Query: Query;
}

/// HRKT for `Query` trait.
pub trait QueryFetch<'a> {
    /// Item type this query type yields.
    type Item: 'a;

    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch: Fetch<'a, Item = Self::Item>;
}

/// Trait to query components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// references to the components and optionally `EntityId` to address same components later.
pub trait Query: for<'a> QueryFetch<'a> + IntoQuery<Query = Self> {
    /// Returns what kind of access the query performs on the component type.
    fn access(&self, ty: TypeId) -> Option<Access>;

    /// Checks if archetype must be skipped.
    fn skip_archetype(&self, archetype: &Archetype) -> bool;

    /// Fetches data from one archetype.
    ///
    /// # Safety
    ///
    /// Must not be called if `skip_archetype` returned `true`.
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> <Self as QueryFetch<'a>>::Fetch;
}

/// Query that does not mutate any components.
///
/// # Safety
///
/// `Query::mutate` must return `false`.
/// `Query` must not borrow components mutably.
/// `Query` must not modify entities versions.
pub unsafe trait ImmutableQuery: Query {}

/// Type alias for items returned by the query type.
pub type QueryItem<'a, Q> = <<Q as IntoQuery>::Query as QueryFetch<'a>>::Item;

/// Merge two optional access values.
pub const fn merge_access(lhs: Option<Access>, rhs: Option<Access>) -> Option<Access> {
    match (lhs, rhs) {
        (None, rhs) => rhs,
        (lhs, None) => lhs,
        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
        _ => Some(Access::Write),
    }
}
