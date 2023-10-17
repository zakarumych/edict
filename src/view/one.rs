use crate::{
    archetype::Archetype,
    entity::{AliveEntity, EntityId, Location},
    epoch::EpochCounter,
    query::{ImmutableQuery, IntoQuery, Query, QueryItem},
    world::World,
};

use super::{expect_match, get_at, BorrowState, RuntimeBorrowState};

/// A view over [`World`] that may be used to access specific components
/// of one entity.
#[must_use]
pub struct ViewOneState<'a, Q: Query, F: Query> {
    query: Q,
    filter: F,
    archetype: &'a Archetype,
    id: EntityId,
    loc: Location,
    borrow: RuntimeBorrowState,
    epochs: &'a EpochCounter,
}

impl<'a, Q: Query, F: Query> Drop for ViewOneState<'a, Q, F> {
    #[inline(always)]
    fn drop(&mut self) {
        self.unlock()
    }
}

impl<'a, Q: Query, F: Query> ViewOneState<'a, Q, F> {
    /// Unlocks runtime borrows.
    /// Allows usage of conflicting views.
    ///
    /// Borrows are automatically unlocked when the view is dropped.
    /// This method is necessary only if caller wants to keep the view
    /// to reuse it later.
    #[inline(always)]
    pub fn unlock(&self) {
        self.borrow.release(
            self.query,
            self.filter,
            core::slice::from_ref(self.archetype),
        )
    }
}

/// View for single entity.
pub type ViewOne<'a, Q, F = ()> =
    ViewOneState<'a, <Q as IntoQuery>::Query, <F as IntoQuery>::Query>;

impl<'a, Q, F> ViewOneState<'a, Q, F>
where
    Q: Query,
    F: Query,
{
    /// Creates a new view over a single entity.
    #[inline(always)]
    pub fn new(world: &'a World, entity: impl AliveEntity, query: Q, filter: F) -> Self {
        let loc = entity.locate(world.entities());
        let archetype = &world.archetypes()[loc.arch as usize];

        ViewOneState {
            query: query.into_query(),
            filter: filter.into_query(),
            archetype,
            id: entity.id(),
            loc,
            borrow: RuntimeBorrowState::new(),
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> ViewOneState<'a, Q, F>
where
    Q: Query,
    F: Query,
{
    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline(always)]
    pub fn get_mut(&mut self) -> Option<QueryItem<Q>> {
        if self.loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, self.id, self.loc.idx);
        }

        // Ensure to borrow view's data.
        self.borrow.acquire(
            self.query,
            self.filter,
            core::slice::from_ref(self.archetype),
        );

        unsafe { self._get() }
    }

    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline(always)]
    #[track_caller]
    pub fn expect_mut(&mut self) -> QueryItem<Q> {
        if self.loc.arch == u32::MAX {
            return expect_match(Query::reserved_entity_item(
                &self.query,
                self.id,
                self.loc.idx,
            ));
        }

        // Ensure to borrow view's data.
        self.borrow.acquire(
            self.query,
            self.filter,
            core::slice::from_ref(self.archetype),
        );

        expect_match(unsafe { self._get() })
    }

    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Otherwise, returns `None`.
    #[inline(always)]
    pub fn map_mut<Fun, R>(&mut self, f: Fun) -> Option<R>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        if self.loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, self.id, self.loc.idx).map(f);
        }

        // Ensure to borrow view's data.
        self.borrow
            .with(self.query, self.filter, self.archetype, || {
                unsafe { self._get() }.map(f)
            })
    }

    #[inline(always)]
    unsafe fn _get(&self) -> Option<QueryItem<Q>> {
        get_at(
            self.query,
            self.filter,
            self.epochs,
            self.archetype,
            self.loc,
        )
    }
}

impl<'a, Q, F> ViewOneState<'a, Q, F>
where
    Q: ImmutableQuery,
    F: ImmutableQuery,
{
    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline(always)]
    pub fn get(&self) -> Option<QueryItem<Q>> {
        if self.loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, self.id, self.loc.idx);
        }

        // Ensure to borrow view's data.
        self.borrow.acquire(
            self.query,
            self.filter,
            core::slice::from_ref(self.archetype),
        );

        unsafe { self._get() }
    }

    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline(always)]
    #[track_caller]
    pub fn expect(&self) -> QueryItem<Q> {
        if self.loc.arch == u32::MAX {
            return expect_match(Query::reserved_entity_item(
                &self.query,
                self.id,
                self.loc.idx,
            ));
        }

        // Ensure to borrow view's data.
        self.borrow.acquire(
            self.query,
            self.filter,
            core::slice::from_ref(self.archetype),
        );

        expect_match(unsafe { self._get() })
    }

    /// Fetches data that matches the view's query and filter
    /// from a bound entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Otherwise, returns `None`.
    #[inline(always)]
    pub fn map<Fun, R>(&self, f: Fun) -> Option<R>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        if self.loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, self.id, self.loc.idx).map(f);
        }

        // Ensure to borrow view's data.
        self.borrow
            .with(self.query, self.filter, self.archetype, || {
                unsafe { self._get() }.map(f)
            })
    }
}
