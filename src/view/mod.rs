//! A view over [`World`] that may be used to access specific components.
//!
//! The world can be seen as a table. Then entities would be rows and components would be columns.
//! And [`View`] is a columnar slice of the table with filtering.
//!
//! [`View`] are parameterized with a query and a filter to select entities and fetch data from them.
//!

use crate::{
    archetype::{chunk_idx, Archetype},
    component::ComponentInfo,
    entity::{EntitySet, Location},
    epoch::EpochCounter,
    query::{AsQuery, Fetch, Query, QueryItem},
    world::World,
    Access,
};

pub use self::{
    borrow::{acquire, release, BorrowState, RuntimeBorrowState, StaticallyBorrowed},
    iter::{
        ViewBatchIter, ViewCellBatchIter, ViewCellIter, ViewIter, ViewValueBatchIter, ViewValueIter,
    },
    one::{ViewOne, ViewOneValue},
};

mod borrow;
mod extend;
mod index;
mod iter;
mod one;

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
#[derive(Clone)]
#[must_use]
pub struct ViewValue<'a, Q: Query, F: Query, B: BorrowState, E> {
    archetypes: &'a [Archetype],
    query: Q,
    filter: F,
    state: B,
    entity_set: &'a EntitySet,
    epochs: &'a EpochCounter,
    extensibility: E,
}

impl<'a, Q, F, B, E> Drop for ViewValue<'a, Q, F, B, E>
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

impl<'a, Q, F, B, E> ViewValue<'a, Q, F, B, E>
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
        self.state.release(self.query, self.filter, self.archetypes);
    }

    #[inline]
    fn with_borrow<R>(&self, arch_idx: u32, f: impl FnOnce() -> R) -> R {
        self.state.with(
            self.query,
            self.filter,
            &self.archetypes[arch_idx as usize],
            f,
        )
    }

    /// Releases borrow state and extracts it.
    #[inline]
    fn extract(self) -> (B, E) {
        self.state.release(self.query, self.filter, self.archetypes);

        let me = core::mem::ManuallyDrop::new(self);
        // Safety: `state` will not be used after this due to `ManuallyDrop`.
        let state = unsafe { core::ptr::read(&me.state) };
        let extensibility = unsafe { core::ptr::read(&me.extensibility) };
        (state, extensibility)
    }
}

impl<'a, Q, F, B, E> ViewValue<'a, Q, F, B, E>
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
    pub fn lock(&mut self) -> View<'_, Q, F> {
        self.acquire_borrow();
        ViewValue {
            archetypes: self.archetypes,
            query: self.query,
            filter: self.filter,
            state: StaticallyBorrowed,
            entity_set: self.entity_set,
            epochs: self.epochs,
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
pub type ViewRef<'a, Q, F = ()> =
    ViewValue<'a, <Q as AsQuery>::Query, <F as AsQuery>::Query, RuntimeBorrowState, Extensible>;

/// View over entities with data fetched using query.
/// Restricted to entities that match both query and filter.
///
/// Returned from [`World::view_*_mut`](crate::world::World::view_mut) methods.
///
/// Statically borrow world mutably to avoid runtime borrow checks.
/// Extensible.
pub type ViewMut<'a, Q, F = ()> =
    ViewValue<'a, <Q as AsQuery>::Query, <F as AsQuery>::Query, StaticallyBorrowed, Extensible>;

/// View over entities with data fetched using query.
/// Restricted to entities that match both query and filter.
///
/// Used as system arguments when views in system conflicts.
/// Use [`View`] instead if there are no conflicts.
///
/// Statically guaranteed to not conflict with views in other parallel systems by system caller.
/// Performs runtime borrow checks of components.
/// Non-extensible.
pub type ViewCell<'a, Q, F = ()> =
    ViewValue<'a, <Q as AsQuery>::Query, <F as AsQuery>::Query, RuntimeBorrowState, NonExtensible>;

/// View over entities with data fetched using query.
/// Restricted to entities that match both query and filter.
///
/// Used as system arguments.
/// If views of a system conflict, use [`ViewCell`].
///
/// Statically guaranteed to not conflict with other views by system caller.
/// Non-extensible.
pub type View<'a, Q, F = ()> =
    ViewValue<'a, <Q as AsQuery>::Query, <F as AsQuery>::Query, StaticallyBorrowed, NonExtensible>;

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
            query,
            filter,
            state,
            entity_set: world.entities(),
            epochs: world.epoch_counter(),
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

impl<'a, Q, F, E> From<ViewValue<'a, Q, F, StaticallyBorrowed, E>>
    for ViewValue<'a, Q, F, RuntimeBorrowState, E>
where
    Q: Query,
    F: Query,
{
    #[inline]
    fn from(view: ViewValue<'a, Q, F, StaticallyBorrowed, E>) -> Self {
        let query = view.query;
        let archetypes = view.archetypes;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (StaticallyBorrowed, extensibility) = view.extract();

        ViewValue {
            archetypes,
            query,
            filter,
            state: RuntimeBorrowState::new(),
            entity_set,
            epochs,
            extensibility,
        }
    }
}

impl<'a, Q, F, B> From<ViewValue<'a, Q, F, B, Extensible>> for ViewValue<'a, Q, F, B, NonExtensible>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline]
    fn from(view: ViewValue<'a, Q, F, B, Extensible>) -> Self {
        let query = view.query;
        let archetypes = view.archetypes;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (state, Extensible) = view.extract();

        ViewValue {
            archetypes,
            query,
            filter,
            state,
            entity_set,
            epochs,
            extensibility: NonExtensible,
        }
    }
}

impl<'a, Q, F> From<ViewValue<'a, Q, F, StaticallyBorrowed, Extensible>>
    for ViewValue<'a, Q, F, RuntimeBorrowState, NonExtensible>
where
    Q: Query,
    F: Query,
{
    #[inline]
    fn from(view: ViewValue<'a, Q, F, StaticallyBorrowed, Extensible>) -> Self {
        let query = view.query;
        let archetypes = view.archetypes;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (StaticallyBorrowed, Extensible) = view.extract();

        ViewValue {
            archetypes,
            query,
            filter,
            state: RuntimeBorrowState::new(),
            entity_set,
            epochs,
            extensibility: NonExtensible,
        }
    }
}

impl<'a, Q, F, B, E> From<ViewValue<'a, (Q,), F, B, E>> for ViewValue<'a, Q, F, B, E>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline]
    fn from(view: ViewValue<'a, (Q,), F, B, E>) -> Self {
        let (query,) = view.query;
        let archetypes = view.archetypes;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (state, extensibility) = view.extract();

        ViewValue {
            archetypes,
            query,
            filter,
            state,
            entity_set,
            epochs,
            extensibility,
        }
    }
}

impl<'a, Q, F, E> From<ViewValue<'a, (Q,), F, StaticallyBorrowed, E>>
    for ViewValue<'a, Q, F, RuntimeBorrowState, E>
where
    Q: Query,
    F: Query,
{
    #[inline]
    fn from(view: ViewValue<'a, (Q,), F, StaticallyBorrowed, E>) -> Self {
        let (query,) = view.query;
        let archetypes = view.archetypes;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (StaticallyBorrowed, extensibility) = view.extract();

        ViewValue {
            archetypes,
            query,
            filter,
            state: RuntimeBorrowState::new(),
            entity_set,
            epochs,
            extensibility,
        }
    }
}

impl<'a, Q, F, B> From<ViewValue<'a, (Q,), F, B, Extensible>>
    for ViewValue<'a, Q, F, B, NonExtensible>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    #[inline]
    fn from(view: ViewValue<'a, (Q,), F, B, Extensible>) -> Self {
        let (query,) = view.query;
        let archetypes = view.archetypes;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (state, Extensible) = view.extract();

        ViewValue {
            archetypes,
            query,
            filter,
            state,
            entity_set,
            epochs,
            extensibility: NonExtensible,
        }
    }
}

impl<'a, Q, F> From<ViewValue<'a, (Q,), F, StaticallyBorrowed, Extensible>>
    for ViewValue<'a, Q, F, RuntimeBorrowState, NonExtensible>
where
    Q: Query,
    F: Query,
{
    #[inline]
    fn from(view: ViewValue<'a, (Q,), F, StaticallyBorrowed, Extensible>) -> Self {
        let (query,) = view.query;
        let archetypes = view.archetypes;
        let filter = view.filter;
        let entity_set = view.entity_set;
        let epochs = view.epochs;
        let (StaticallyBorrowed, Extensible) = view.extract();

        ViewValue {
            archetypes,
            query,
            filter,
            state: RuntimeBorrowState::new(),
            entity_set,
            epochs,
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
unsafe fn get_at<'a, Q, F>(
    query: Q,
    filter: F,
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

    if archetype.is_empty() {
        return None;
    }

    if !query.visit_archetype(archetype) || !filter.visit_archetype(archetype) {
        return None;
    }

    if !unsafe { query.visit_archetype_late(archetype) }
        || !unsafe { filter.visit_archetype_late(archetype) }
    {
        return None;
    }

    let epoch = epochs.next_if(Q::Query::MUTABLE || F::Query::MUTABLE);

    let mut query_fetch = unsafe { query.fetch(arch, archetype, epoch) };

    if !unsafe { Fetch::visit_chunk(&mut query_fetch, chunk_idx(idx)) } {
        return None;
    }

    unsafe { Fetch::touch_chunk(&mut query_fetch, chunk_idx(idx)) }

    if !unsafe { Fetch::visit_item(&mut query_fetch, idx) } {
        return None;
    }

    let mut filter_fetch = unsafe { filter.fetch(arch, archetype, epoch) };

    if !unsafe { Fetch::visit_chunk(&mut filter_fetch, chunk_idx(idx)) } {
        return None;
    }

    unsafe { Fetch::touch_chunk(&mut filter_fetch, chunk_idx(idx)) }

    if !unsafe { Fetch::visit_item(&mut filter_fetch, idx) } {
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
fn validate_query_filter<Q, F>(query: Q, filter: F, archetype: &Archetype)
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
