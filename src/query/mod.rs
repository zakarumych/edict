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

use core::any::TypeId;

use crate::{
    archetype::Archetype, component::ComponentInfo, entity::EntityId, epoch::EpochId, Access,
};

pub use self::{
    alt::{Alt, FetchAlt},
    // any_of::AnyOf,
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
    filter::{FilteredFetch, Not, With, Without},
    modified::{
        Modified, ModifiedFetchAlt, ModifiedFetchCopied, ModifiedFetchRead, ModifiedFetchWith,
        ModifiedFetchWrite,
    },
    read::{FetchRead, Read},
    with_epoch::{EpochOf, FetchEpoch},
    write::{FetchWrite, Write},
};

mod alt;
// mod any_of;
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

/// Types associated with a query type.
pub trait AsQuery {
    /// Associated query type.
    type Query: Query;
}

/// Types convertible into query type.
pub trait IntoQuery: AsQuery {
    /// Converts into query.
    fn into_query(self) -> Self::Query;
}

/// Types convertible into query type.
pub unsafe trait IntoSendQuery: IntoQuery + AsSendQuery {}
unsafe impl<Q> IntoSendQuery for Q where Q: IntoQuery + AsSendQuery {}

/// Types associated with default-constructible query type.
pub trait DefaultQuery: AsQuery {
    /// Returns default query instance.
    fn default_query() -> Self::Query;
}

/// Types associated with default-constructible query type.
pub unsafe trait DefaultSendQuery: DefaultQuery + AsSendQuery {}
unsafe impl<Q> DefaultSendQuery for Q where Q: DefaultQuery + AsSendQuery {}

/// Detected write aliasing.
/// Should be either resolved at runtime or reported with panic.
pub struct WriteAlias;

/// Trait to query components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// references to the components and optionally [`EntityId`] to address same components later.
///
/// [`EntityId`]: edict::entity::EntityId
pub unsafe trait Query: IntoQuery<Query = Self> + Copy + Send + Sync + 'static {
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
    /// This method may return stronger access type if it is impossible to know
    /// exact access with only type-id.
    #[must_use]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias>;

    /// Checks if archetype must be visited or skipped.
    /// If returns `false`, `access_archetype` and `fetch` must not be called,
    /// meaning that complex query should either skip archetype entirely or
    /// for this query specifically.
    ///
    /// If this method returns `true`, `access_archetype` and `fetch` must be safe to call.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

    /// Asks query to provide types and access for the specific archetype.
    /// Must call provided closure with type id and access pairs.
    /// Only types from archetype must be used to call closure.
    ///
    /// # Safety
    ///
    /// Must not be called if `visit_archetype` returned `false`.
    /// Implementation are allowed to assume conditions that make `visit_archetype` return `true`.
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access));

    /// Checks if archetype must be visited or skipped a second time after
    /// required access was granted.
    ///
    /// Most queries do not check visiting again so defaults to `true`.
    #[must_use]
    #[inline(always)]
    fn visit_archetype_late(&self, archetype: &Archetype) -> bool {
        debug_assert!(self.visit_archetype(archetype));
        let _ = archetype;
        true
    }

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

    /// Returns item for reserved entity if reserved entity (no components) satisfies the query.
    /// Otherwise returns `None`.
    #[must_use]
    #[inline(always)]
    fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<Self::Item<'a>> {
        let _ = id;
        let _ = idx;
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

/// Query that can be used from non-main thread.
pub unsafe trait SendQuery: Query {}

pub unsafe trait SendImmutableQuery: SendQuery + ImmutableQuery {}
unsafe impl<Q> SendImmutableQuery for Q where Q: SendQuery + ImmutableQuery {}

/// Query that can be used from non-main thread.
pub unsafe trait AsSendQuery: AsQuery {}

unsafe impl<Q> AsSendQuery for Q
where
    Q: AsQuery,
    Q::Query: SendQuery,
{
}

/// Type alias for items returned by the [`Query`] type.
pub type QueryItem<'a, Q> = <<Q as AsQuery>::Query as Query>::Item<'a>;
