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
    entities::{Entities, EntitiesFetch},
    fetch::{Fetch, UnitFetch, VerifyFetch},
    filter::{Filter, FilteredFetch, FilteredQuery, IntoFilter, With, Without},
    iter::QueryIter,
    modified::{Modified, ModifiedFetchAlt, ModifiedFetchRead, ModifiedFetchWrite},
    phantom::{ImmutablePhantomQuery, PhantomQuery, PhantomQueryFetch},
    read::{read, FetchRead},
    write::{write, FetchWrite},
};

mod alt;
mod borrow;
mod entities;
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
    type Query: Query + IntoQuery<Query = Self::Query>;
}

/// HRKT for [`Query`] trait.
pub trait QueryFetch<'a> {
    /// Item type this query type yields.
    type Item: 'a;

    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch: Fetch<'a, Item = Self::Item>;
}

/// Trait to query components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// references to the components and optionally [`EntityId`] to address same components later.
///
/// [`EntityId`]: edict::entity::EntityId
pub unsafe trait Query: for<'a> QueryFetch<'a> + IntoQuery<Query = Self> {
    /// Returns what kind of access the query performs on the component type.
    fn access(&self, ty: TypeId) -> Option<Access>;

    /// Checks if archetype must be skipped.
    fn skip_archetype(&self, archetype: &Archetype) -> bool;

    /// Asks query to provide types and access for the specific archetype.
    /// Must call provided closure with type id and access pairs.
    /// For each `(id, access)` pair access must match one returned from `access` method for the same id.
    /// Only types from archetype must be used to call closure.
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access));

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

/// Wraps mutable reference to query and implement query.
pub struct MutQuery<'a, T: 'a> {
    query: &'a mut T,
}

impl<'a, T> From<&'a mut T> for MutQuery<'a, T> {
    fn from(query: &'a mut T) -> Self {
        MutQuery { query }
    }
}

impl<'a, T> MutQuery<'a, T> {
    /// Wraps mutable reference to query.
    pub fn new(query: &'a mut T) -> Self {
        MutQuery { query }
    }

    /// Unwraps query.
    pub fn into_inner(self) -> &'a mut T {
        self.query
    }
}

impl<'a, T> QueryFetch<'a> for MutQuery<'_, T>
where
    T: Query,
{
    type Item = <T as QueryFetch<'a>>::Item;
    type Fetch = <T as QueryFetch<'a>>::Fetch;
}

impl<T> IntoQuery for MutQuery<'_, T>
where
    T: Query,
{
    type Query = Self;
}

unsafe impl<T> Query for MutQuery<'_, T>
where
    T: Query,
{
    fn access(&self, ty: TypeId) -> Option<Access> {
        self.query.access(ty)
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        self.query.skip_archetype(archetype)
    }

    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        self.query.access_archetype(archetype, f)
    }

    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> <Self as QueryFetch<'a>>::Fetch {
        self.query.fetch(archetype, epoch)
    }
}

unsafe impl<T> ImmutableQuery for MutQuery<'_, T> where T: ImmutableQuery {}

/// Query that does not mutate any components.
///
/// # Safety
///
/// [`Query`] must not borrow components mutably.
/// [`Query`] must not modify entities versions.
pub unsafe trait ImmutableQuery: Query {}

/// Type alias for items returned by the [`Query`] type.
pub type QueryItem<'a, Q> = <<Q as IntoQuery>::Query as QueryFetch<'a>>::Item;

/// Merge two optional access values.
#[inline]
pub const fn merge_access(lhs: Option<Access>, rhs: Option<Access>) -> Option<Access> {
    match (lhs, rhs) {
        (None, rhs) => rhs,
        (lhs, None) => lhs,
        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
        _ => Some(Access::Write),
    }
}
