use core::mem::MaybeUninit;

use crate::{
    archetype::Archetype,
    entity::{AliveEntity, EntityId, Location},
    epoch::EpochCounter,
    query::{AsQuery, ImmutableQuery, Query, QueryItem},
    world::World,
};

use super::{expect_match, get_at, BorrowState, RuntimeBorrowState};

/// A view over [`World`] that may be used to access specific components
/// of one entity.
#[must_use]
pub struct ViewOneValue<'a, Q: Query, F: Query> {
    query: Q,
    filter: F,

    // Init if loc.arch != u32::MAX
    archetype: MaybeUninit<&'a Archetype>,
    id: EntityId,
    loc: Location,
    borrow: RuntimeBorrowState,
    epochs: &'a EpochCounter,
}

impl<'a, Q: Query, F: Query> Drop for ViewOneValue<'a, Q, F> {
    #[inline]
    fn drop(&mut self) {
        self.unlock()
    }
}

impl<'a, Q: Query, F: Query> ViewOneValue<'a, Q, F> {
    /// Unlocks runtime borrows.
    /// Allows usage of conflicting views.
    ///
    /// Borrows are automatically unlocked when the view is dropped.
    /// This method is necessary only if caller wants to keep the view
    /// to reuse it later.
    #[inline]
    pub fn unlock(&self) {
        if self.loc.arch == u32::MAX {
            return;
        }

        // Safety: archetype is init if loc.arch != u32::MAX
        let archetype = unsafe { self.archetype.assume_init() };

        self.borrow
            .release(self.query, self.filter, core::slice::from_ref(archetype))
    }
}

/// View for single entity.
pub type ViewOne<'a, Q, F = ()> = ViewOneValue<'a, <Q as AsQuery>::Query, <F as AsQuery>::Query>;

impl<'a, Q, F> ViewOneValue<'a, Q, F>
where
    Q: Query,
    F: Query,
{
    /// Creates a new view over a single entity.
    #[inline]
    pub fn new(world: &'a World, entity: impl AliveEntity, query: Q, filter: F) -> Self {
        let loc = entity.locate(world.entities());
        let mut archetype = MaybeUninit::uninit();

        if loc.arch != u32::MAX {
            archetype.write(&world.archetypes()[loc.arch as usize]);
        }

        ViewOneValue {
            query,
            filter,
            archetype,
            id: entity.id(),
            loc,
            borrow: RuntimeBorrowState::new(),
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> ViewOneValue<'a, Q, F>
where
    Q: Query,
    F: Query,
{
    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline]
    pub fn get_mut(&mut self) -> Option<QueryItem<'_, Q>> {
        if self.loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, self.id, self.loc.idx);
        }

        // Safety: archetype is init if loc.arch != u32::MAX
        let archetype = unsafe { self.archetype.assume_init() };

        // Ensure to borrow view's data.
        self.borrow
            .acquire(self.query, self.filter, core::slice::from_ref(archetype));

        unsafe { get_at(self.query, self.filter, self.epochs, archetype, self.loc) }
    }

    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// # Panics
    ///
    /// Panics if entity does not match the view's query and filter.
    #[inline]
    #[track_caller]
    pub fn expect_mut(&mut self) -> QueryItem<'_, Q> {
        expect_match(self.get_mut())
    }

    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Otherwise, returns `None`.
    #[inline]
    pub fn map_mut<Fun, R>(&mut self, f: Fun) -> Option<R>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        if self.loc.arch == u32::MAX {
            let item = Query::reserved_entity_item(&self.query, self.id, self.loc.idx);
            return item.map(f);
        }

        // Safety: archetype is init if loc.arch != u32::MAX
        let archetype = unsafe { self.archetype.assume_init() };

        // Ensure to borrow view's data.
        self.borrow.with(self.query, self.filter, archetype, || {
            let item = unsafe { get_at(self.query, self.filter, self.epochs, archetype, self.loc) };
            item.map(f)
        })
    }
}

impl<'a, Q, F> ViewOneValue<'a, Q, F>
where
    Q: ImmutableQuery,
    F: ImmutableQuery,
{
    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline]
    pub fn get(&self) -> Option<QueryItem<'_, Q>> {
        if self.loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, self.id, self.loc.idx);
        }

        // Safety: archetype is init if loc.arch != u32::MAX
        let archetype = unsafe { self.archetype.assume_init() };

        // Ensure to borrow view's data.
        self.borrow
            .acquire(self.query, self.filter, core::slice::from_ref(archetype));

        unsafe { get_at(self.query, self.filter, self.epochs, archetype, self.loc) }
    }

    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline]
    #[track_caller]
    pub fn expect(&self) -> QueryItem<'_, Q> {
        expect_match(self.get())
    }

    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Otherwise, returns `None`.
    #[inline]
    pub fn map<Fun, R>(&self, f: Fun) -> Option<R>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        if self.loc.arch == u32::MAX {
            let item = Query::reserved_entity_item(&self.query, self.id, self.loc.idx);
            return item.map(f);
        }

        // Safety: archetype is init if loc.arch != u32::MAX
        let archetype = unsafe { self.archetype.assume_init() };

        // Ensure to borrow view's data.
        self.borrow.with(self.query, self.filter, archetype, || {
            let item = unsafe { get_at(self.query, self.filter, self.epochs, archetype, self.loc) };
            item.map(f)
        })
    }
}
