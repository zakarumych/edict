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

use core::{any::TypeId, ops::Deref};

use alloc::vec::Vec;
use hashbrown::HashMap;

use crate::{
    Access, archetype::Archetype, component::ComponentInfo, entity::EntityId, epoch::EpochId,
    hash::NoOpHasherBuilder,
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
        ModifiedFetchWrite, ModifiedRelationFetch,
    },
    read::{FetchRead, Read},
    with_epoch::{EpochOf, FetchEpoch, WithEpoch},
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

#[cfg(test)]
mod tests;

/// Types associated with a query type.
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a query type",
    note = "If `{Self}` is a component type, use `&{Self}` or `&mut {Self}` instead"
)]
pub trait AsQuery {
    /// Associated query type.
    type Query: Query;
}

/// Detected write aliasing.
/// Should be either resolved at runtime or reported with panic.
pub struct WriteAlias;

/// Trait to query components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// references to the components and optionally [`EntityId`] to address same components later.
///
/// [`EntityId`]: edict::entity::EntityId
///
/// # Safety
///
/// Implementations must adhere to unsafe contract of unsafe methods.
#[diagnostic::on_unimplemented(label = "`{Self}` is not a query type")]
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

    /// Returns the archetype visit result that can be cached for this query type.
    ///
    /// Returns `None` if checking the query instance is required on each pass.
    ///
    /// `Some(v)` must equal [`Self::visit_archetype`] for every instance of this
    /// query type while the archetype's component and borrow metadata is unchanged.
    /// This caches only the early check; [`Self::visit_archetype_late`] must still run.
    #[must_use]
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool>;

    /// Returns what kind of access the query performs on the component type.
    /// This method may return stronger access type if it is impossible to know
    /// exact access with only type-id.
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
    ///
    /// # Safety
    ///
    /// Must not be called if `visit_archetype` returned `false`.
    /// access_archetype must have been called before this method.
    #[must_use]
    #[inline(always)]
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

/// Type alias for items returned by the [`Query`] type.
pub type QueryItem<'a, Q> = <<Q as AsQuery>::Query as Query>::Item<'a>;

/// Hack around inability to say `: Query<for<'a> Fetch<'a> = Self::BatchFetch<'a>>`
#[doc(hidden)]
pub trait BatchQueryHack<'a>: Query<Fetch<'a> = Self::BatchFetchHack> {
    /// Associated batch type.
    type BatchHack: 'a;

    /// Associated batch fetch type.
    type BatchFetchHack: BatchFetch<'a, Batch = Self::BatchHack> + 'a;
}

/// Extension trait for [`Query`] to provide additional methods to views.
pub trait BatchQuery: for<'a> BatchQueryHack<'a, BatchHack = Self::Batch<'a>> {
    /// Associated batch type.
    type Batch<'a>: 'a;
}

impl<'a, Q> BatchQueryHack<'a> for Q
where
    Q: Query,
    Q::Fetch<'a>: BatchFetch<'a>,
{
    type BatchHack = <Q::Fetch<'a> as BatchFetch<'a>>::Batch;
    type BatchFetchHack = Q::Fetch<'a>;
}

impl<Q> BatchQuery for Q
where
    Q: Query,
    for<'a> Q::Fetch<'a>: BatchFetch<'a>,
{
    type Batch<'a> = <Q::Fetch<'a> as BatchFetch<'a>>::Batch;
}

/// Type alias for items returned by the [`Query`] type.
pub type QueryBatch<'a, Q> = <<Q as AsQuery>::Query as BatchQuery>::Batch<'a>;

/// Types convertible into query type.
pub trait IntoQuery: AsQuery {
    /// Converts into query.
    fn into_query(self) -> Self::Query;
}

/// Default-constructible query type.
#[diagnostic::on_unimplemented(label = "`{Self}` is not a default-constructible query type")]
pub trait DefaultQuery: Query {
    /// Returns default query.
    fn default_query() -> Self;
}

/// Types associated with default-constructible query type.
#[diagnostic::on_unimplemented(label = "`{Self}` is not a default-constructible query type")]
pub trait AsDefaultQuery: AsQuery<Query: DefaultQuery> {
    /// Returns default query.
    #[inline]
    fn default_query() -> Self::Query {
        DefaultQuery::default_query()
    }
}

impl<Q> AsDefaultQuery for Q
where
    Q: AsQuery,
    Q::Query: DefaultQuery,
{
}

/// Query that can be used from non-main thread.
///
/// # Safety
///
/// Query type must be safely usable from non-main thread.
/// This includes ensuring that any queried components
/// are `Sync` if queried immutably and `Send` if queried mutably.
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a query type that can be used from non-main thread"
)]
pub unsafe trait SendQuery: Query {}

/// Query that can be used from non-main thread.
///
/// # Safety
///
/// Associated query type must implement [`SendQuery`].
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a query type that can be used from non-main thread"
)]
pub unsafe trait AsSendQuery: AsQuery<Query: SendQuery> {}

unsafe impl<Q> AsSendQuery for Q
where
    Q: AsQuery,
    Q::Query: SendQuery,
{
}

/// Types convertible into query type that can be used from non-main thread.
pub trait IntoSendQuery: IntoQuery + AsSendQuery {}
impl<Q> IntoSendQuery for Q where Q: IntoQuery + AsSendQuery {}

/// Query that does not mutate any components.
///
/// # Safety
///
/// [`Query`] must not borrow components mutably.
/// [`Query`] must not modify entities versions.
#[diagnostic::on_unimplemented(label = "`{Self}` mutates components")]
pub unsafe trait ImmutableQuery: Query {
    /// Checks that query is valid in compile time.
    const CHECK_VALID: () = {
        if Self::MUTABLE {
            panic!("Immutable query cannot fetch mutable components");
        }
    };
}

/// Type associated with a query type that does not mutate any components.
#[diagnostic::on_unimplemented(label = "`{Self}` mutates components")]
pub trait AsImmutableQuery: AsQuery<Query: ImmutableQuery> {}

impl<Q> AsImmutableQuery for Q
where
    Q: AsQuery,
    Q::Query: ImmutableQuery,
{
}

/// Types convertible into query type that can be used from non-main thread.
pub trait IntoImmutableQuery: IntoQuery + AsImmutableQuery {}
impl<Q> IntoImmutableQuery for Q where Q: IntoQuery + AsImmutableQuery {}

/// Default-constructible query type that can be used from non-main thread.
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a default-constructible query type or cannot be used from non-main thread"
)]
pub trait DefaultSendQuery: DefaultQuery + SendQuery {}

impl<Q> DefaultSendQuery for Q where Q: DefaultQuery + SendQuery {}

/// Types associated with default-constructible query type that can be used from non-main thread.
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a default-constructible query type or cannot be used from non-main thread"
)]
pub trait AsDefaultSendQuery: AsQuery<Query: DefaultSendQuery> {}

impl<Q> AsDefaultSendQuery for Q
where
    Q: AsQuery,
    Q::Query: DefaultQuery + SendQuery,
{
}

/// Default-constructible query type that can be used from non-main thread.
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a default-constructible query type or mutates components"
)]
pub trait DefaultImmutableQuery: DefaultQuery + ImmutableQuery {}

impl<Q> DefaultImmutableQuery for Q where Q: DefaultQuery + ImmutableQuery {}

/// Types associated with default-constructible query type that can be used from non-main thread.
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a default-constructible query type or mutates components"
)]
pub trait AsDefaultImmutableQuery: AsQuery<Query: DefaultImmutableQuery> {}

impl<Q> AsDefaultImmutableQuery for Q
where
    Q: AsQuery,
    Q::Query: DefaultQuery + ImmutableQuery,
{
}

/// Query that does not mutate any components and can be used from non-main thread.
#[diagnostic::on_unimplemented(
    label = "`{Self}` mutates components or cannot be used from non-main thread"
)]
pub trait SendImmutableQuery: SendQuery + ImmutableQuery {}
impl<Q> SendImmutableQuery for Q where Q: SendQuery + ImmutableQuery {}

/// Type associated with a query type that does not mutate any components and can be used from non-main thread.
#[diagnostic::on_unimplemented(
    label = "`{Self}` mutates components or cannot be used from non-main thread"
)]
pub trait AsSendImmutableQuery: AsQuery<Query: SendImmutableQuery> {}

impl<Q> AsSendImmutableQuery for Q
where
    Q: AsQuery,
    Q::Query: SendImmutableQuery,
{
}

/// Types convertible into query type that can be used from non-main thread and does not mutate any components.
#[diagnostic::on_unimplemented(
    label = "`{Self}` mutates components or cannot be used from non-main thread"
)]
pub trait IntoSendImmutableQuery: IntoQuery + AsSendImmutableQuery {}
impl<Q> IntoSendImmutableQuery for Q where Q: IntoQuery + AsSendImmutableQuery {}

/// Default-constructible query type that can be used from non-main thread.
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a default-constructible query type or cannot be used from non-main thread"
)]
pub trait DefaultSendImmutableQuery: DefaultQuery + SendQuery + ImmutableQuery {}

impl<Q> DefaultSendImmutableQuery for Q where Q: DefaultQuery + SendQuery + ImmutableQuery {}

/// Types associated with default-constructible query type that can be used from non-main thread.
#[diagnostic::on_unimplemented(
    label = "`{Self}` is not a default-constructible query type or cannot be used from non-main thread"
)]
pub trait AsDefaultSendImmutableQuery: AsQuery<Query: DefaultSendImmutableQuery> {}

impl<Q> AsDefaultSendImmutableQuery for Q
where
    Q: AsQuery,
    Q::Query: DefaultQuery + SendQuery + ImmutableQuery,
{
}

#[derive(Default)]
struct OneQueryCache {
    /// 2-bit per archetype visiting cache.
    /// 0 - unknown
    /// 1 - check
    /// 2 - skip
    /// 3 - visit
    bits: Vec<u64>,
    archetypes: usize,
}

/// Stores archetype visit decisions by query type.
pub(crate) struct QueryCache {
    queries: HashMap<TypeId, OneQueryCache, NoOpHasherBuilder>,
}

impl QueryCache {
    pub fn new() -> Self {
        QueryCache {
            queries: HashMap::with_hasher(NoOpHasherBuilder),
        }
    }

    /// Return QueryCache for query type.
    pub fn init<Q>(&mut self, archetypes: &[Archetype])
    where
        Q: Query,
    {
        let cache = self.queries.entry(TypeId::of::<Q>()).or_default();

        if cache.bits.len() * 32 < archetypes.len() {
            cache.bits.resize(archetypes.len().div_ceil(32), 0);
        }

        for (idx, archetype) in archetypes[cache.archetypes..].iter().enumerate() {
            let idx = idx + cache.archetypes;

            let ci = idx / 32;
            let bi = idx % 32;

            let bits = match Q::cache_visit_archetype(archetype) {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            };

            cache.bits[ci] |= bits << (bi * 2);
        }

        cache.archetypes = archetypes.len();
    }

    pub fn get<Q>(&self, query: Q) -> QueryCacheRef<'_, Q>
    where
        Q: Query,
    {
        let bits = match self.queries.get(&TypeId::of::<Q>()) {
            Some(cache) => &cache.bits[..],
            None => &[],
        };

        QueryCacheRef { bits, query }
    }
}

/// Owns a query value and borrows its early archetype visit decisions.
///
/// Dereferences to the owned query for fetching, late checks, and other query methods.
/// The inherent [`Self::visit_archetype`] method uses the cache. To invoke the
/// query's early check directly, use `(*handle).visit_archetype(archetype)`.
/// Cached indices refer to the original world's complete archetype slice.
/// Late archetype checks and entity filtering still run independently.
#[derive(Clone, Copy)]
pub struct QueryCacheRef<'a, Q> {
    bits: &'a [u64],
    query: Q,
}

impl<Q> Deref for QueryCacheRef<'_, Q> {
    type Target = Q;

    fn deref(&self) -> &Q {
        &self.query
    }
}

impl<'a, Q> QueryCacheRef<'a, Q>
where
    Q: Query,
{
    #[inline]
    /// Returns an uncached handle that checks the query on every visit.
    pub fn empty(query: Q) -> Self {
        Self { bits: &[], query }
    }

    #[inline]
    pub(crate) fn into_tuple(self) -> QueryCacheRef<'a, (Q,)> {
        QueryCacheRef {
            bits: self.bits,
            query: (self.query,),
        }
    }

    #[inline]
    /// Checks an archetype using its original index in the world's archetype slice.
    pub fn visit_archetype(&self, archetype: &Archetype, idx: u32) -> bool {
        let ci = idx / 32;
        let bi = idx % 32;

        let bits = (self.bits.get(ci as usize).copied().unwrap_or_default() >> (bi * 2)) & 3;

        match bits {
            0 => self.query.visit_archetype(archetype),
            1 => false,
            2 => true,
            _ => false, // Actually unreachable.
        }
    }
}

impl<'a, Q: Query> QueryCacheRef<'a, (Q,)> {
    pub(crate) fn unwrap_tuple(self) -> QueryCacheRef<'a, Q> {
        QueryCacheRef {
            bits: self.bits,
            query: self.query.0,
        }
    }
}
