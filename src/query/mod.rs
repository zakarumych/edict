//! Queries and iterators.
//!
//! To efficiently iterate over entities with specific set of components,
//! or only over those where specific component is modified, or missing,
//! [`Query`] is the solution.
//!
//! [`Query`] trait has a lot of implementations and is composable using tuples.

use core::any::TypeId;

use crate::{archetype::Archetype, entity::EntityId, epoch::EpochId};

pub use self::{
    alt::{Alt, FetchAlt},
    any_of::AnyOf,
    boolean::{
        And, And2, And3, And4, And5, And6, And7, And8, BooleanFetch, BooleanFetchOp, BooleanQuery,
        Or, Or2, Or3, Or4, Or5, Or6, Or7, Or8, Xor, Xor2, Xor3, Xor4, Xor5, Xor6, Xor7, Xor8,
    },
    borrow::{
        FetchBorrowAllRead, FetchBorrowAnyRead, FetchBorrowAnyWrite, FetchBorrowOneRead,
        FetchBorrowOneWrite, QueryBorrowAll, QueryBorrowAny, QueryBorrowOne,
    },
    copied::{copied, Copied, FetchCopied},
    entities::{Entities, EntitiesFetch, EntitiesQuery},
    fetch::{Fetch, UnitFetch, VerifyFetch},
    filter::{FilteredFetch, FilteredQuery, Not, With, Without},
    iter::QueryIter,
    modified::{
        Modified, ModifiedFetchAlt, ModifiedFetchCopied, ModifiedFetchRead, ModifiedFetchWith,
        ModifiedFetchWrite,
    },
    phantom::{ImmutablePhantomQuery, PhantomQuery},
    read::{read, FetchRead, Read},
    with_epoch::{EpochOf, FetchEpoch},
    write::{write, FetchWrite, Write},
};

mod alt;
mod any_of;
mod boolean;
mod borrow;
mod copied;
mod entities;
mod fetch;
mod filter;
mod iter;
mod modified;
mod option;
mod phantom;
mod read;
mod tuple;
mod with_epoch;
mod write;

/// Specifies kind of access query performs for particular component.
#[derive(Clone, Copy, Debug)]
pub enum Access {
    /// Cannot be aliased with any other access.
    Write,

    /// Shared access to component. Can be aliased with other [`Access::Read`] accesses.
    Read,
    // /// Temporary access to component. Can be aliased with any other in the same query.
    // /// For different queries acts like [`Access::Read`].
    // /// Queries with this access type produce output not tied to component borrow.
    // Touch,
}

/// Types associated with a query type.
pub trait IntoQuery {
    /// Associated query type.
    type Query: Query;

    /// Converts into query.
    fn into_query(self) -> Self::Query;
}

/// Types associated with default-constructible query type.
pub trait DefaultQuery: IntoQuery {
    /// Converts into query.
    fn default_query() -> Self::Query;
}

/// Trait to query components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// references to the components and optionally [`EntityId`] to address same components later.
///
/// [`EntityId`]: edict::entity::EntityId
pub unsafe trait Query: IntoQuery<Query = Self> {
    /// Item type this query type yields.
    type Item<'a>: 'a;

    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch<'a>: Fetch<'a, Item = Self::Item<'a>> + 'a;

    /// Returns what kind of access the query performs on the component type.
    #[must_use]
    fn access(&self, ty: TypeId) -> Option<Access>;

    /// Checks if archetype must be visited or skipped.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

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
    #[must_use]
    unsafe fn fetch<'a>(&mut self, archetype: &'a Archetype, epoch: EpochId) -> Self::Fetch<'a>;

    /// Returns item for reserved entity if reserved entity satisfies the query.
    /// Otherwise returns `None`.
    #[must_use]
    #[inline]
    fn reserved_entity_item<'a>(&self, id: EntityId) -> Option<Self::Item<'a>> {
        drop(id);
        None
    }
}

/// Wraps mutable reference to query and implement query for it.
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

impl<T> IntoQuery for MutQuery<'_, T>
where
    T: Query,
{
    type Query = Self;

    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for MutQuery<'_, T>
where
    T: Query,
{
    type Item<'a> = T::Item<'a>;
    type Fetch<'a> = T::Fetch<'a>;

    fn access(&self, ty: TypeId) -> Option<Access> {
        self.query.access(ty)
    }

    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.query.visit_archetype(archetype)
    }

    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        self.query.access_archetype(archetype, f)
    }

    unsafe fn fetch<'a>(&mut self, archetype: &'a Archetype, epoch: EpochId) -> Self::Fetch<'a> {
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
pub type QueryItem<'a, Q> = <<Q as IntoQuery>::Query as Query>::Item<'a>;

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

/// Helps to assert that type implements [`Query`] in compile time.
const fn assert_query<Q: Query>() {}

/// Helps to assert that type implements [`ImmutableQuery`] in compile time.
const fn assert_immutable_query<Q: ImmutableQuery>() {
    assert_query::<Q>();
}
