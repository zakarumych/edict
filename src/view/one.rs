use crate::{
    archetype::Archetype,
    entity::{AliveEntity, EntityId, EntitySet, Location},
    epoch::EpochCounter,
    query::{IntoQuery, Query, QueryItem},
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
    entity_set: &'a EntitySet,
    borrow: RuntimeBorrowState,
    epochs: &'a EpochCounter,
}

pub type ViewOne<'a, Q, F = ()> =
    ViewOneState<'a, <Q as IntoQuery>::Query, <F as IntoQuery>::Query>;

impl<'a, Q, F> ViewOneState<'a, Q, F>
where
    Q: Query + Default,
    F: Query + Default,
{
    #[inline(always)]
    pub fn new(world: &World, entity: impl AliveEntity) -> Self {
        let loc = entity.locate(world.entity_set());
        let archetype = &world.archetypes()[loc.arch as usize];

        ViewOneState {
            query: Q::default(),
            filter: F::default(),
            archetype,
            id: entity.id(),
            loc,
            entity_set: world.entity_set(),
            borrow: RuntimeBorrowState::new(),
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> ViewOneState<'a, (Q,), F>
where
    Q: Query,
    F: Query + Default,
{
    #[inline(always)]
    pub fn with_query(
        world: &World,
        entity: impl AliveEntity,
        query: impl IntoQuery<Query = Q>,
    ) -> Self {
        let loc = entity.locate(world.entity_set());
        let archetype = &world.archetypes()[loc.arch as usize];

        ViewOneState {
            query: (query.into_query(),),
            filter: F::default(),
            archetype,
            id: entity.id(),
            loc,
            entity_set: world.entity_set(),
            borrow: RuntimeBorrowState::new(),
            epochs: world.epoch_counter(),
        }
    }
}

impl<'a, Q, F> ViewOneState<'a, (Q,), (F,)>
where
    Q: Query,
    F: Query,
{
    #[inline(always)]
    pub fn with_query_filter(
        world: &World,
        entity: impl AliveEntity,
        query: impl IntoQuery<Query = Q>,
        filter: impl IntoQuery<Query = F>,
    ) -> Self {
        let loc = entity.locate(world.entity_set());
        let archetype = &world.archetypes()[loc.arch as usize];

        ViewOneState {
            query: (query.into_query(),),
            filter: (filter.into_query(),),
            archetype,
            id: entity.id(),
            loc,
            entity_set: world.entity_set(),
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
    pub fn get(&self) -> Option<QueryItem<Q>> {
        if self.loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, self.id, self.loc.idx);
        }

        // Ensure to borrow view's data.
        self.borrow.acquire(
            &self.query,
            &self.filter,
            core::slice::from_ref(self.archetype),
        );

        self._get()
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
            &self.query,
            &self.filter,
            core::slice::from_ref(self.archetype),
        );

        expect_match(self._get())
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
            .with(&self.query, &self.filter, self.archetype, || {
                self._get().map(f)
            })
    }

    #[inline(always)]
    fn _get(&self) -> Option<QueryItem<Q>> {
        get_at(
            &self.query,
            &self.filter,
            self.epochs,
            self.archetype,
            self.loc,
        )
    }
}
