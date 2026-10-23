//! A view over [`World`] that may be used to access specific components.
//!
//! The world can be seen as a table. Then entities would be rows and components would be columns.
//! And [`View`] is a columnar slice of the table with filtering.
//!
//! [`View`] are parameterized with a query and a filter to select entities and fetch data from them.
//!

use crate::{
    Access,
    archetype::{Archetype, chunk_idx},
    component::ComponentInfo,
    entity::{EntitySet, Location},
    epoch::{EpochCounter, EpochSource, LocalEpochCounter},
    query::{AsQuery, Fetch, Query, QueryCache, QueryCacheRef, QueryItem},
    world::World,
};

pub use self::{
    borrow::{BorrowState, RuntimeBorrowState, StaticallyBorrowed, acquire, release},
    iter::{
        ViewBatchIter, ViewCellBatchIter, ViewCellIter, ViewIter, ViewValueBatchIter, ViewValueIter,
    },
    one::{LocalViewOne, ViewOne, ViewOneValue},
};

mod borrow;
mod cache;
mod extend;
mod index;
mod iter;
mod one;

#[cfg(test)]
mod tests;

/// Flag indicating that view is extensible.
#[derive(Copy, Clone)]
pub struct Extensible;

/// Flag indicating that view is non-extensible.
#[derive(Copy, Clone)]
pub struct NonExtensible;

impl From<Extensible> for NonExtensible {
    #[inline]
    fn from(_: Extensible) -> Self {
        NonExtensible
    }
}

/// A view over [`World`] that may be used to access specific components.
/// Indexed access retains its last fetch in a `Cell`, so views are not `Sync`.
///
/// ```compile_fail
/// use edict::view::View;
/// fn sync<T: Sync>() {}
/// sync::<View<'static, &'static u32>>();
/// ```
#[must_use]
pub struct ViewValue<'a, Q: Query, F: Query, B: BorrowState, E, C: EpochSource = EpochCounter> {
    archetypes: &'a [Archetype],
    cache: &'a QueryCache,
    query: QueryCacheRef<'a, Q>,

    filter: QueryCacheRef<'a, F>,

    state: B,
    entity_set: &'a EntitySet,
    epochs: &'a C,
    last_fetches: cache::FetchCache<cache::IndexedFetches<'a, Q, F>>,
    extensibility: E,
}

impl<'a, Q, F, B, E, C: EpochSource> Drop for ViewValue<'a, Q, F, B, E, C>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline]
    fn drop(&mut self) {
        self.release_borrow();
    }
}

impl<'a, Q, F, B, E, C: EpochSource> ViewValue<'a, Q, F, B, E, C>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline]
    fn acquire_borrow(&self) {
        self.state.acquire(self.query, self.filter, self.archetypes);
    }

    #[inline]
    fn release_borrow(&self) {
        self.last_fetches.assert_idle();
        self.clear_fetch_cache();
        self.state.release(self.query, self.filter, self.archetypes);
    }

    #[inline]
    fn clear_fetch_cache(&self) {
        self.last_fetches.clear();
    }

    #[inline]
    fn with_borrow<R>(&self, arch_idx: u32, f: impl FnOnce() -> R) -> R {
        self.clear_fetch_cache();
        self.state.with(
            self.query,
            self.filter,
            &self.archetypes[arch_idx as usize],
            arch_idx,
            || {
                let _clear = cache::ClearOnDrop(&self.last_fetches);
                f()
            },
        )
    }

    /// Releases borrow state and extracts it.
    #[inline]
    fn extract(self) -> (B, E) {
        self.release_borrow();

        let me = core::mem::ManuallyDrop::new(self);
        // Safety: `state` will not be used after this due to `ManuallyDrop`.
        let state = unsafe { core::ptr::read(&me.state) };
        let extensibility = unsafe { core::ptr::read(&me.extensibility) };
        (state, extensibility)
    }
}

impl<'a, Q, F, B, E, C: EpochSource> ViewValue<'a, Q, F, B, E, C>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    /// Locks query borrows.
    ///
    /// Allows to construct statically borrowed view with
    /// lifetime tied to this view borrow.
    ///
    /// Helpful to satisfy API that requires [`View`].
    ///
    /// For symmetry this method is available for statically and exclusively borrowed views too.
    #[inline]
    pub fn lock(&mut self) -> View<'_, Q, F, C> {
        self.clear_fetch_cache();
        self.acquire_borrow();
        ViewValue {
            archetypes: self.archetypes,
            cache: self.cache,
            query: self.query,
            filter: self.filter,
            state: StaticallyBorrowed,
            entity_set: self.entity_set,
            epochs: self.epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility: NonExtensible,
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
    #[inline]
    pub fn unlock(&mut self) {
        self.release_borrow()
    }
}

/// View over entities with data fetched using query.
/// Restricted to entities that match both query and filter.
///
/// Returned from [`World::view_*`](crate::world::World::view) methods.
///
/// Performs runtime borrow checks of components.
/// Extensible.
pub type ViewRef<'a, Q, F = (), C = EpochCounter> =
    ViewValue<'a, <Q as AsQuery>::Query, <F as AsQuery>::Query, RuntimeBorrowState, Extensible, C>;

/// View over entities with data fetched using query.
/// Restricted to entities that match both query and filter.
///
/// Returned from [`World::view_*_mut`](crate::world::World::view_mut) methods.
///
/// Statically borrow world mutably to avoid runtime borrow checks.
/// Extensible.
pub type ViewMut<'a, Q, F = (), C = EpochCounter> =
    ViewValue<'a, <Q as AsQuery>::Query, <F as AsQuery>::Query, StaticallyBorrowed, Extensible, C>;

/// View over entities with data fetched using query.
/// Restricted to entities that match both query and filter.
///
/// Used as system arguments when views in system conflicts.
/// Use [`View`] instead if there are no conflicts.
///
/// Statically guaranteed to not conflict with views in other parallel systems by system caller.
/// Performs runtime borrow checks of components.
/// Non-extensible.
pub type ViewCell<'a, Q, F = (), C = EpochCounter> = ViewValue<
    'a,
    <Q as AsQuery>::Query,
    <F as AsQuery>::Query,
    RuntimeBorrowState,
    NonExtensible,
    C,
>;

/// View over entities with data fetched using query.
/// Restricted to entities that match both query and filter.
///
/// Used as system arguments.
/// If views of a system conflict, use [`ViewCell`].
///
/// Statically guaranteed to not conflict with other views by system caller.
/// Non-extensible.
pub type View<'a, Q, F = (), C = EpochCounter> = ViewValue<
    'a,
    <Q as AsQuery>::Query,
    <F as AsQuery>::Query,
    StaticallyBorrowed,
    NonExtensible,
    C,
>;

impl<'a, Q, F, B, E> ViewValue<'a, Q, F, B, E>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    /// Creates a new view over the world without checks.
    ///
    /// Uses user-provided query, filter, borrow state and extensibility flag.
    ///
    /// # Safety
    ///
    /// User is responsible to ensure that view won't create mutable aliasing of entity components.
    #[inline]
    pub unsafe fn new_unchecked(
        world: &'a World,
        query: Q,
        filter: F,
        state: B,
        extensibility: E,
    ) -> Self {
        ViewValue {
            archetypes: world.archetypes(),
            cache: world.query_cache(),
            query: world.get_query_cache(query),
            filter: world.get_query_cache(filter),
            state,
            entity_set: world.entities(),
            epochs: world.epoch_counter(),
            last_fetches: cache::FetchCache::new(),
            extensibility,
        }
    }
}

impl<'a, Q, F> ViewValue<'a, Q, F, StaticallyBorrowed, Extensible>
where
    Q: Query,
    F: Query,
{
    /// Creates a new view over the world.
    /// Borrows the world mutably, so no other views can be created.
    /// In exchange it does not require runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline]
    pub fn new_mut(world: &'a mut World, query: Q, filter: F) -> Self {
        for archetype in world.archetypes() {
            validate_query_filter(query, filter, archetype);
        }

        unsafe { ViewValue::new_unchecked(world, query, filter, StaticallyBorrowed, Extensible) }
    }
}

impl<'a, Q, F> ViewValue<'a, Q, F, RuntimeBorrowState, Extensible>
where
    Q: Query,
    F: Query,
{
    /// Creates a new view over the world.
    /// Performs runtime borrow checks.
    ///
    /// Uses user-provided query and filter.
    #[inline]
    pub fn new_ref(world: &'a World, query: Q, filter: F) -> Self {
        unsafe {
            ViewValue::new_unchecked(world, query, filter, RuntimeBorrowState::new(), Extensible)
        }
    }
}

impl<'a, Q, F, E, C: EpochSource> From<ViewValue<'a, Q, F, StaticallyBorrowed, E, C>>
    for ViewValue<'a, Q, F, RuntimeBorrowState, E, C>
where
    Q: Query,
    F: Query,
{
    #[inline]
    fn from(view: ViewValue<'a, Q, F, StaticallyBorrowed, E, C>) -> Self {
        let query = view.query;
        let archetypes = view.archetypes;
        let cache = view.cache;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (StaticallyBorrowed, extensibility) = view.extract();

        ViewValue {
            archetypes,
            cache,
            query,
            filter,
            state: RuntimeBorrowState::new(),
            entity_set,
            epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility,
        }
    }
}

impl<'a, Q, F, B, C: EpochSource> From<ViewValue<'a, Q, F, B, Extensible, C>>
    for ViewValue<'a, Q, F, B, NonExtensible, C>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline]
    fn from(view: ViewValue<'a, Q, F, B, Extensible, C>) -> Self {
        let query = view.query;
        let archetypes = view.archetypes;
        let cache = view.cache;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (state, Extensible) = view.extract();

        ViewValue {
            archetypes,
            cache,
            query,
            filter,
            state,
            entity_set,
            epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility: NonExtensible,
        }
    }
}

impl<'a, Q, F, C: EpochSource> From<ViewValue<'a, Q, F, StaticallyBorrowed, Extensible, C>>
    for ViewValue<'a, Q, F, RuntimeBorrowState, NonExtensible, C>
where
    Q: Query,
    F: Query,
{
    #[inline]
    fn from(view: ViewValue<'a, Q, F, StaticallyBorrowed, Extensible, C>) -> Self {
        let query = view.query;
        let archetypes = view.archetypes;
        let cache = view.cache;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (StaticallyBorrowed, Extensible) = view.extract();

        ViewValue {
            archetypes,
            cache,
            query,
            filter,
            state: RuntimeBorrowState::new(),
            entity_set,
            epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility: NonExtensible,
        }
    }
}

impl<'a, Q, F, B, E, C: EpochSource> From<ViewValue<'a, (Q,), F, B, E, C>>
    for ViewValue<'a, Q, F, B, E, C>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline]
    fn from(view: ViewValue<'a, (Q,), F, B, E, C>) -> Self {
        let query = view.query.unwrap_tuple();
        let archetypes = view.archetypes;
        let cache = view.cache;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (state, extensibility) = view.extract();

        ViewValue {
            archetypes,
            cache,
            query,
            filter,
            state,
            entity_set,
            epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility,
        }
    }
}

impl<'a, Q, F, E, C: EpochSource> From<ViewValue<'a, (Q,), F, StaticallyBorrowed, E, C>>
    for ViewValue<'a, Q, F, RuntimeBorrowState, E, C>
where
    Q: Query,
    F: Query,
{
    #[inline]
    fn from(view: ViewValue<'a, (Q,), F, StaticallyBorrowed, E, C>) -> Self {
        let query = view.query.unwrap_tuple();
        let archetypes = view.archetypes;
        let cache = view.cache;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (StaticallyBorrowed, extensibility) = view.extract();

        ViewValue {
            archetypes,
            cache,
            query,
            filter,
            state: RuntimeBorrowState::new(),
            entity_set,
            epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility,
        }
    }
}

impl<'a, Q, F, B, C: EpochSource> From<ViewValue<'a, (Q,), F, B, Extensible, C>>
    for ViewValue<'a, Q, F, B, NonExtensible, C>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline]
    fn from(view: ViewValue<'a, (Q,), F, B, Extensible, C>) -> Self {
        let query = view.query.unwrap_tuple();
        let archetypes = view.archetypes;
        let cache = view.cache;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (state, Extensible) = view.extract();

        ViewValue {
            archetypes,
            cache,
            query,
            filter,
            state,
            entity_set,
            epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility: NonExtensible,
        }
    }
}

impl<'a, Q, F, C: EpochSource> From<ViewValue<'a, (Q,), F, StaticallyBorrowed, Extensible, C>>
    for ViewValue<'a, Q, F, RuntimeBorrowState, NonExtensible, C>
where
    Q: Query,
    F: Query,
{
    #[inline]
    fn from(view: ViewValue<'a, (Q,), F, StaticallyBorrowed, Extensible, C>) -> Self {
        let query = view.query.unwrap_tuple();
        let archetypes = view.archetypes;
        let cache = view.cache;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (StaticallyBorrowed, Extensible) = view.extract();

        ViewValue {
            archetypes,
            cache,
            query,
            filter,
            state: RuntimeBorrowState::new(),
            entity_set,
            epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility: NonExtensible,
        }
    }
}

#[inline]
#[track_caller]
fn expect_match<T>(value: Option<T>) -> T {
    value.expect("Entity does not match view's query and filter")
}

#[inline]
#[track_caller]
fn expect_alive<T>(value: Option<T>) -> T {
    value.expect("Entity is not alive")
}

#[inline]
unsafe fn get_at<'a, Q, F, C: EpochSource>(
    query: QueryCacheRef<'a, Q>,
    filter: QueryCacheRef<'a, F>,
    epochs: &C,
    archetype: &'a Archetype,
    loc: Location,
) -> Option<QueryItem<'a, Q>>
where
    Q: Query,
    F: Query,
{
    let Location { arch, idx } = loc;
    assert!(idx < archetype.len(), "Wrong location");

    if archetype.is_empty() {
        return None;
    }

    if !filter.visit_archetype(archetype, arch) || !query.visit_archetype(archetype, arch) {
        return None;
    }

    if !unsafe { filter.visit_archetype_late(archetype) }
        || !unsafe { query.visit_archetype_late(archetype) }
    {
        return None;
    }

    let epoch = epochs.next_if(Q::Query::MUTABLE || F::Query::MUTABLE);

    let mut filter_fetch = unsafe { filter.fetch(arch, archetype, epoch) };

    if !unsafe { Fetch::visit_chunk(&mut filter_fetch, chunk_idx(idx)) } {
        return None;
    }

    unsafe { Fetch::touch_chunk(&mut filter_fetch, chunk_idx(idx)) }

    if !unsafe { Fetch::visit_item(&mut filter_fetch, idx) } {
        return None;
    }

    let mut query_fetch = unsafe { query.fetch(arch, archetype, epoch) };

    if !unsafe { Fetch::visit_chunk(&mut query_fetch, chunk_idx(idx)) } {
        return None;
    }

    unsafe { Fetch::touch_chunk(&mut query_fetch, chunk_idx(idx)) }

    if !unsafe { Fetch::visit_item(&mut query_fetch, idx) } {
        return None;
    }

    Some(unsafe { Fetch::get_item(&mut query_fetch, idx) })
}

#[track_caller]
#[inline]
fn has_conflict_query_filter<Q, F>(query: Q, filter: F, comp: &ComponentInfo) -> bool
where
    Q: Query,
    F: Query,
{
    let Ok(q) = query.component_access(comp) else {
        return true;
    };
    let Ok(f) = filter.component_access(comp) else {
        return true;
    };

    match (q, f) {
        (None, _) | (_, None) => false,
        (Some(Access::Read), Some(Access::Read)) => false,
        (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write)) => true,
    }
}

#[track_caller]
#[inline]
pub(crate) fn validate_query_filter<Q, F>(query: Q, filter: F, archetype: &Archetype)
where
    Q: Query,
    F: Query,
{
    for comp in archetype.infos() {
        if has_conflict_query_filter(query, filter, comp) {
            mutable_alias_in_view(comp.name());
        }
    }
}

#[inline(never)]
#[cold]
fn mutable_alias_in_view(comp: &str) -> ! {
    panic!("Mutable alias of `{comp}` in a view");
}

impl<'a, Q: Query, F: Query, B: BorrowState + Clone, E: Clone, C: EpochSource> Clone
    for ViewValue<'a, Q, F, B, E, C>
{
    fn clone(&self) -> Self {
        Self {
            archetypes: self.archetypes,
            cache: self.cache,
            query: self.query,
            filter: self.filter,
            state: self.state.clone(),
            entity_set: self.entity_set,
            epochs: self.epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility: self.extensibility.clone(),
        }
    }
}

impl<'a, Q: Query, F: Query, B: BorrowState, E, C: EpochSource> ViewValue<'a, Q, F, B, E, C> {
    /// Creates a view with an explicit epoch source under the caller's capability.
    ///
    /// # Safety
    /// The caller must guarantee component access and exclusive local epoch mode
    /// for the entire lifetime of views and iterators derived from this source.
    #[inline]
    pub(crate) unsafe fn new_with_epochs(
        world: &'a World,
        query: Q,
        filter: F,
        state: B,
        extensibility: E,
        epochs: &'a C,
    ) -> Self {
        Self {
            archetypes: world.archetypes(),
            cache: world.query_cache(),
            query: world.get_query_cache(query),
            filter: world.get_query_cache(filter),
            state,
            entity_set: world.entities(),
            epochs,
            last_fetches: cache::FetchCache::new(),
            extensibility,
        }
    }
}

/// Runtime-borrowed view using exclusive local-world epochs.
pub type LocalViewRef<'a, Q, F = ()> = ViewRef<'a, Q, F, LocalEpochCounter>;
/// Statically borrowed view using exclusive local-world epochs.
pub type LocalView<'a, Q, F = ()> = View<'a, Q, F, LocalEpochCounter>;
/// Mutable-world view using exclusive local-world epochs.
pub type LocalViewMut<'a, Q, F = ()> = ViewMut<'a, Q, F, LocalEpochCounter>;
/// Runtime-borrowed non-extensible view using local-world epochs.
pub type LocalViewCell<'a, Q, F = ()> = ViewCell<'a, Q, F, LocalEpochCounter>;
