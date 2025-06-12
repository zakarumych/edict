//! Queries are used to fetch data from the [`World`].
//!
//! Queries implement [`Query`] trait and are passed into methods by value.
//!
//! For convenience, `AsQuery` and `IntoQuery` traits are implemented for some types
//! to be used instead of queries in generic parameters.
//! For example [`Read<T>`] is a query to fetch `T` for reading, but `&T` implements [`AsQuery`]
//!
//! [`IntoQuery`] extends this to add conversion from type to a query carrying the state.
//! This trait is used extensively in the API to pass query by value.
//!
//! Stateless queries and some stateful queries with default state implement [`DefaultQuery`].
//! This trait is used extensively in the API to specify query type.
//!
//! Queries can be combined into tuples producing a new query that yields
//! a tuple of items from the original queries and filtering out entities
//! that don't satisfy all queries.
//!
//! Query can be used with [`World`] to produce a [`View`] parameterized with the query.
//! A [`View`] can be iterated to visit all matching entities and fetch
//! data from them.
//! [`View`] can also be indexed with [`Entity`] to fetch data from
//! a specific entity.
//! [`View`]s can also be used as function-system arguments.
//!
//! [`World`]: crate::world::World
//! [`View`]: crate::view::View
//! [`Entity`]: crate::entity::Entity
//!

use core::any::TypeId;

use crate::{
    archetype::Archetype, component::ComponentInfo, entity::EntityId, epoch::EpochId, Access,
};

pub use self::{
    alt::{Alt, FetchAlt, RefMut},
    // any_of::AnyOf,
    boolean::{
        And, And2, And3, And4, And5, And6, And7, And8, BooleanFetch, BooleanFetchOp, BooleanQuery,
        Or, Or2, Or3, Or4, Or5, Or6, Or7, Or8, Xor, Xor2, Xor3, Xor4, Xor5, Xor6, Xor7, Xor8,
    },
    borrow::{
        BorrowAll, BorrowAny, BorrowOne, FetchBorrowAllRead, FetchBorrowAnyRead,
        FetchBorrowAnyWrite, FetchBorrowOneRead, FetchBorrowOneWrite,
    },
    copied::{Cpy, FetchCpy},
    entities::{Entities, EntitiesFetch},
    fetch::{BatchFetch, Fetch, UnitFetch, VerifyFetch},
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
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a query type",
    note = "If `{Self}` is a component type, use `&{Self}` or `&mut {Self}` instead"
)]
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
#[diagnostic::on_unimplemented(label = "`{Self}` is not a stateless query type")]
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

    /// Set to `true` if query may return mutable references to components.
    const MUTABLE: bool;

    /// Set to `true` if query filters individual entities.
    ///
    /// If set `false` - `Fetch` must unconditionally return `true` for all valid calls to
    /// `Fetch::visit_chunk` and `Fetch::visit_item`.
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
    #[inline]
    unsafe fn visit_archetype_late(&self, archetype: &Archetype) -> bool {
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
    #[inline]
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

/// Query that does not mutate any components and can be used from non-main thread.
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

/// Hack around inability to say `: Query<for<'a> Fetch<'a> = Self::BatchFetch<'a>>`
#[doc(hidden)]
pub unsafe trait BatchQueryHack<'a>: Query<Fetch<'a> = Self::BatchFetchHack> {
    /// Associated batch type.
    type BatchHack: 'a;

    /// Associated batch fetch type.
    type BatchFetchHack: BatchFetch<'a, Batch = Self::BatchHack> + 'a;
}

/// Extension trait for [`Query`] to provide additional methods to views.
pub unsafe trait BatchQuery:
    for<'a> BatchQueryHack<'a, BatchHack = Self::Batch<'a>>
{
    /// Associated batch type.
    type Batch<'a>: 'a;
}

unsafe impl<'a, Q> BatchQueryHack<'a> for Q
where
    Q: Query,
    Q::Fetch<'a>: BatchFetch<'a>,
{
    type BatchHack = <Q::Fetch<'a> as BatchFetch<'a>>::Batch;
    type BatchFetchHack = Q::Fetch<'a>;
}

unsafe impl<Q> BatchQuery for Q
where
    Q: Query,
    for<'a> Q::Fetch<'a>: BatchFetch<'a>,
{
    type Batch<'a> = <Q::Fetch<'a> as BatchFetch<'a>>::Batch;
}

/// Type alias for items returned by the [`Query`] type.
pub type QueryBatch<'a, Q> = <<Q as AsQuery>::Query as BatchQuery>::Batch<'a>;
