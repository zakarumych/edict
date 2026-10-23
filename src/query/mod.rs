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
    /// Allows retaining this query's fetch between indexed accesses.
    ///
    /// Opt-in implementations must retain only stable raw access state with no
    /// resource-owning drop effects. They must reconstruct each returned fetch
    /// with the requested lifetime and reset all per-access traversal state.
    const SUPPORTS_FETCH_REUSE: bool = false;

    /// Archetype acceptance and optional-fetch presence remain invariant while
    /// the query instance and archetype schema are unchanged, even when component
    /// values or modification epochs change. Both early and late checks must
    /// satisfy this guarantee. Optional wrappers may reuse their stored presence
    /// without repeating those checks when this is true.
    const FETCH_REUSE_MATCHES_STABLE: bool = false;

    /// Rebuilds an indexed fetch from retained native access state.
    ///
    /// The default performs an ordinary fetch. Unsupported queries are never
    /// retained by views. Supported queries must reproduce fresh-fetch effects
    /// for a new epoch; handing off a newly prepared fetch at its existing epoch
    /// must not repeat constructor effects.
    ///
    /// # Safety
    /// `cached` must come from this same query instance, live world and archetype.
    /// Its archetype layout and backing allocations must remain valid under
    /// continuously held query/filter access; values and epochs may change. No item borrowed from an earlier access may remain
    /// live when access can conflict. Early and late checks must have passed. Reusing retained state requires
    /// `Self::SUPPORTS_FETCH_REUSE` to be true.
    /// The epoch must be fresh, or match a newly prepared fetch handed off before
    /// its first item access. Short-lived fetches must be dropped before reuse.
    /// Returned items may be used only while the retained access remains valid.
    /// Returned fetches must not borrow the cached object itself. They may only
    /// borrow independently valid world storage.
    /// Retained state must not borrow mutable component data through references;
    /// opt-in implementations reconstruct native access state explicitly.
    #[inline]
    unsafe fn reuse_fetch<'cached, 'a>(
        &self,
        cached: &mut Self::Fetch<'cached>,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Self::Fetch<'a> {
        let _ = cached;
        unsafe { self.fetch(arch_idx, archetype, epoch) }
    }

    /// Number of internal metadata slots prepared per archetype.
    #[doc(hidden)]
    const FETCH_CACHE_SLOTS: usize = 0;

    /// Prepares internal metadata slots for one archetype; never stores payload pointers.
    #[doc(hidden)]
    fn prepare_fetch_cache(archetype: &Archetype, slots: &mut [FetchCacheSlot]) {
        let _ = (archetype, slots);
    }

    /// Fetches using metadata prepared for this exact live archetype and query layout.
    ///
    /// # Safety
    /// The usual fetch access requirements apply. Slots must come from a fully
    /// initialized cache row belonging to this archetype and Self's slot layout.
    #[doc(hidden)]
    #[inline]
    unsafe fn fetch_cached<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
        slots: &[FetchCacheSlot],
    ) -> Self::Fetch<'a> {
        let _ = slots;
        unsafe { self.fetch(arch_idx, archetype, epoch) }
    }

    /// Optional fetching with prepared metadata. Zero-slot queries preserve their
    /// existing optional-fetch overrides and effects.
    ///
    /// # Safety
    /// Slots must match this live archetype and query layout. Required component
    /// access must be granted even when the query rejects the archetype.
    #[doc(hidden)]
    #[inline]
    unsafe fn fetch_optional_cached<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
        slots: &[FetchCacheSlot],
    ) -> Option<Self::Fetch<'a>> {
        if Self::FETCH_CACHE_SLOTS == 0 {
            return unsafe { self.fetch_optional(arch_idx, archetype, epoch) };
        }
        if self.visit_archetype(archetype) && unsafe { self.visit_archetype_late(archetype) } {
            Some(unsafe { self.fetch_cached(arch_idx, archetype, epoch, slots) })
        } else {
            None
        }
    }

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

    /// Prepares a fetch if this archetype passes the early and late checks.
    ///
    /// Returns `None` when either check rejects the archetype. Overrides must
    /// preserve the checks' results and the effects of [`Self::fetch`], including
    /// modification tracking with the supplied epoch.
    ///
    /// # Safety
    ///
    /// The caller must hold the component access required by this query for
    /// this archetype, even though the archetype may fail the early check.
    /// Any returned fetch must obey the same access and lifetime requirements
    /// as [`Self::fetch`].
    #[must_use]
    #[inline]
    unsafe fn fetch_optional<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Option<Self::Fetch<'a>> {
        if self.visit_archetype(archetype) && unsafe { self.visit_archetype_late(archetype) } {
            Some(unsafe { self.fetch(arch_idx, archetype, epoch) })
        } else {
            None
        }
    }

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

/// Opaque storage for prepared archetype metadata. This is an internal query hook.
#[doc(hidden)]
#[derive(Clone, Copy, Default)]
pub struct FetchCacheSlot {
    component: Option<core::ptr::NonNull<crate::archetype::ArchetypeComponent>>,
}

// Safety: pointers only address immutable, schema-stable metadata records. Their
// ComponentData remains behind UnsafeCell; fetching payload still requires the
// original Query access grant and reads the current data header each time.
unsafe impl Send for FetchCacheSlot {}
unsafe impl Sync for FetchCacheSlot {}

impl FetchCacheSlot {
    #[inline]
    pub(crate) fn component(archetype: &Archetype, id: TypeId) -> Self {
        Self {
            component: archetype.component(id).map(core::ptr::NonNull::from),
        }
    }

    /// # Safety
    /// This slot must have been prepared for this exact, still-live archetype.
    /// Its schema must remain fixed; component access must be granted separately.
    #[inline]
    pub(crate) unsafe fn get<'a>(
        &self,
        owner: &'a Archetype,
    ) -> Option<&'a crate::archetype::ArchetypeComponent> {
        let _ = owner;
        self.component
            .map(|component| unsafe { component.as_ref() })
    }
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
    slots: Vec<FetchCacheSlot>,
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

    pub fn init<Q: Query>(&mut self, archetypes: &[Archetype]) {
        let cache = self.queries.entry(TypeId::of::<Q>()).or_default();
        let published_slots = cache
            .archetypes
            .checked_mul(Q::FETCH_CACHE_SLOTS)
            .expect("query cache layout overflow");
        // A panicking prepare hook leaves its row unpublished. Discard its partial
        // slots before retrying; published rows and their visit bits remain valid.
        cache.slots.truncate(published_slots);
        if cache.bits.len() * 32 < archetypes.len() {
            cache.bits.resize(archetypes.len().div_ceil(32), 0);
        }
        for idx in cache.archetypes..archetypes.len() {
            let start = cache.slots.len();
            let end = start
                .checked_add(Q::FETCH_CACHE_SLOTS)
                .expect("query cache layout overflow");
            cache.slots.resize(end, FetchCacheSlot::default());
            Q::prepare_fetch_cache(&archetypes[idx], &mut cache.slots[start..end]);
            let bits = match Q::cache_visit_archetype(&archetypes[idx]) {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            };
            let shift = (idx % 32) * 2;
            cache.bits[idx / 32] = (cache.bits[idx / 32] & !(3 << shift)) | (bits << shift);
            cache.archetypes = idx + 1;
        }
    }

    pub fn get<Q: Query>(&self, query: Q) -> QueryCacheRef<'_, Q> {
        let (bits, slots, archetypes) = match self.queries.get(&TypeId::of::<Q>()) {
            Some(cache) => (
                &cache.bits[..],
                &cache.slots[..cache.archetypes * Q::FETCH_CACHE_SLOTS],
                cache.archetypes,
            ),
            None => (&[][..], &[][..], 0),
        };
        QueryCacheRef {
            bits,
            slots,
            archetypes,
            query,
        }
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
    slots: &'a [FetchCacheSlot],
    archetypes: usize,
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
        Self {
            bits: &[],
            slots: &[],
            archetypes: 0,
            query,
        }
    }

    #[inline]
    pub(crate) fn into_tuple(self) -> QueryCacheRef<'a, (Q,)> {
        QueryCacheRef {
            bits: self.bits,
            slots: self.slots,
            archetypes: self.archetypes,
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

    /// Rebuilds a fetch from native state retained under the same live access.
    ///
    /// # Safety
    /// All requirements of [`Query::reuse_fetch`] apply.
    #[inline]
    pub unsafe fn reuse_fetch<'cached, 'w>(
        &self,
        cached: &mut Q::Fetch<'cached>,
        arch_idx: u32,
        archetype: &'w Archetype,
        epoch: EpochId,
    ) -> Q::Fetch<'w> {
        unsafe { self.query.reuse_fetch(cached, arch_idx, archetype, epoch) }
    }
    /// Fetches with prepared metadata when the row is fully published.
    ///
    /// # Safety
    /// The index and archetype must belong to this handle's original live world,
    /// with the same query slot layout. Query fetch access requirements still apply.
    #[inline]
    pub unsafe fn fetch<'w>(
        &self,
        arch_idx: u32,
        archetype: &'w Archetype,
        epoch: EpochId,
    ) -> Q::Fetch<'w> {
        if Q::FETCH_CACHE_SLOTS != 0 && (arch_idx as usize) < self.archetypes {
            let start = arch_idx as usize * Q::FETCH_CACHE_SLOTS;
            let slots = &self.slots[start..start + Q::FETCH_CACHE_SLOTS];
            unsafe { self.query.fetch_cached(arch_idx, archetype, epoch, slots) }
        } else {
            unsafe { self.query.fetch(arch_idx, archetype, epoch) }
        }
    }
}

impl<'a, Q: Query> QueryCacheRef<'a, (Q,)> {
    pub(crate) fn unwrap_tuple(self) -> QueryCacheRef<'a, Q> {
        QueryCacheRef {
            bits: self.bits,
            slots: self.slots,
            archetypes: self.archetypes,
            query: self.query.0,
        }
    }
}
