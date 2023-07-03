//! Queries are used to fetch data from the [`World`].
//!
//! Basic query types are `&T`, `&mut T`, and `Entities`.
//! `&T` fetches component `T` for reading. It yields `&T`, so it also
//! filters out entities that don't have component `T`.
//! `&mut T` fetches component `T` for writing. And filters same way as `&T`.
//! `Entities` fetches [`EntityId`]s. All entities have an ID,
//! so it doesn't filter anything.
//!
//! Queries are divided into two categories: stateful and stateless.
//!
//! Stateful queries implement `Query` trait and are passed into methods by value.
//! Some of them have `DefaultQuery` implementation, and can be used in methods
//! that do not accept query as an argument and only as a type parameter.
//!
//! Stateless queries implement `PhantomQuery` trait, and `PhantomData<Q>`
//! is stateful counterpart for stateless query `Q`.
//!
//! All basic queries are stateless.
//! Advanced queries like `Modified` - that filter entities based on
//! component modification - and `RelatedTo` - that filter entities based on
//! entity relation - are stateful.
//!
//! Queries can be combined into tuples producing a new query that yields
//! a tuple of items from the original queries and filtering out entities
//! that don't satisfy all queries.
//!
//! TODO: Derive impl for structures with named fields.
//!
//!
//! Queries can be used with [`World`] to produce a [`View`].
//! A [`View`] can be iterated to visit all matching entities and fetch
//! data from them.
//! [`View`] can also be indexed with [`Entity`] to fetch data from
//! a specific entity.
//!

use core::any::{type_name, TypeId};

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
    copied::{Cpy, FetchCopied},
    entities::{Entities, EntitiesFetch},
    fetch::{Fetch, UnitFetch, VerifyFetch},
    filter::{FilteredFetch, FilteredQuery, Not, With, Without},
    modified::{
        Modified, ModifiedFetchAlt, ModifiedFetchCopied, ModifiedFetchRead, ModifiedFetchWith,
        ModifiedFetchWrite,
    },
    read::{FetchRead, Read},
    with_epoch::{EpochOf, FetchEpoch},
    write::{FetchWrite, Write},
};

mod alt;
mod any_of;
mod boolean;
mod borrow;
mod copied;
mod entities;
mod fetch;
mod filter;
mod modified;
mod option;
// mod phantom;
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
    /// Returns default query instance.
    fn default_query() -> Self::Query;
}

/// Trait to query components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// references to the components and optionally [`EntityId`] to address same components later.
///
/// [`EntityId`]: edict::entity::EntityId
pub unsafe trait Query: IntoQuery<Query = Self> + 'static {
    /// Item type this query type yields.
    type Item<'a>: 'a;

    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch<'a>: Fetch<'a, Item = Self::Item<'a>> + 'a;

    /// Set to `true` if query fetches at least one mutable component.
    const MUTABLE: bool;

    /// Set to `true` if query filters individual entities.
    const FILTERS_ENTITIES: bool = false;

    /// Returns what kind of access the query performs on the component type.
    #[must_use]
    fn access(&self, ty: TypeId) -> Option<Access>;

    /// Checks if archetype must be visited or skipped.
    ///
    /// This method must be safe to execute in parallel with any other accesses
    /// to the same archetype.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

    /// Asks query to provide types and access for the specific archetype.
    /// Must call provided closure with type id and access pairs.
    /// For each `(id, access)` pair access must match one returned from `access` method for the same id.
    /// Only types from archetype must be used to call closure.
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: impl FnMut(TypeId, Access));

    /// Fetches data from one archetype.
    ///
    /// # Safety
    ///
    /// Must not be called if `visit_archetype` returned `false`.
    #[must_use]
    unsafe fn fetch<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Self::Fetch<'a>;

    /// Returns item for reserved entity if reserved entity satisfies the query.
    /// Otherwise returns `None`.
    #[must_use]
    #[inline(always)]
    fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<Self::Item<'a>> {
        drop(id);
        drop(idx);
        None
    }
}

/// Query that does not mutate any components.
///
/// # Safety
///
/// [`Query`] must not borrow components mutably.
/// [`Query`] must not modify entities versions.
pub unsafe trait ImmutableQuery: Query {
    /// Checks that query is valid in compile time.
    const CHECK_VALID: () = {
        if Self::MUTABLE {
            panic!("Immutable query cannot fetch mutable components");
        }
    };
}

/// Type alias for items returned by the [`Query`] type.
pub type QueryItem<'a, Q> = <<Q as IntoQuery>::Query as Query>::Item<'a>;

/// Error type returned by try_merge_access if write aliasing is detected.
pub struct WriteAliasing;

/// Merge two optional access values.
#[inline(always)]
pub const fn try_merge_access(
    lhs: Option<Access>,
    rhs: Option<Access>,
) -> Result<Option<Access>, WriteAliasing> {
    match (lhs, rhs) {
        (None, one) | (one, None) => Ok(one),
        (Some(Access::Read), Some(Access::Read)) => Ok(Some(Access::Read)),
        _ => Err(WriteAliasing),
    }
}

/// Merge two optional access values.
#[inline(always)]
pub fn merge_access<T: ?Sized>(lhs: Option<Access>, rhs: Option<Access>) -> Option<Access> {
    match (lhs, rhs) {
        (None, one) | (one, None) => one,
        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
        _ => panic!("Write aliasing detected in query: {}", type_name::<T>()),
    }
}
