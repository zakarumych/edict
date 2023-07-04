use core::ops::{Index, IndexMut};

use crate::{
    entity::{AliveEntity, Entity, Location},
    query::{Query, QueryItem, Read, Write},
    view::get_at,
};

use super::{expect_alive, expect_match, BorrowState, ViewValue};

impl<Q, F, B> ViewValue<'_, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline(always)]
    pub fn get(&self, entity: impl AliveEntity) -> Option<QueryItem<Q>> {
        let loc = entity.locate(self.entity_set);

        if loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, entity.id(), loc.idx);
        }

        // Ensure to borrow view's data.
        self.borrow
            .acquire(&self.query, &self.filter, self.archetypes);

        self._get(loc)
    }

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline(always)]
    #[track_caller]
    pub fn expect(&self, entity: impl Entity) -> QueryItem<Q> {
        let loc = expect_alive(entity.lookup(self.entity_set));

        if loc.arch == u32::MAX {
            return expect_match(Query::reserved_entity_item(
                &self.query,
                entity.id(),
                loc.idx,
            ));
        }

        // Ensure to borrow view's data.
        self.borrow
            .acquire(&self.query, &self.filter, self.archetypes);

        expect_match(self._get(loc))
    }

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Otherwise, calls closure with `None`.
    #[inline(always)]
    pub fn map<Fun, R>(&self, entity: impl AliveEntity, f: Fun) -> Option<R>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        let loc = entity.locate(self.entity_set);

        if loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, entity.id(), loc.idx).map(f);
        }

        let archetype = &self.archetypes[loc.arch as usize];

        // Ensure to borrow view's data.
        self.borrow.with(&self.query, &self.filter, archetype, || {
            self._get(loc).map(f)
        })
    }

    #[inline(always)]
    fn _get(&self, loc: Location) -> Option<QueryItem<Q>> {
        debug_assert_ne!(loc.arch, u32::MAX);

        let archetype = &self.archetypes[loc.arch as usize];
        get_at(self.query, self.filter, self.epochs, archetype, loc)
    }
}

impl<E, T, F, B> Index<E> for ViewValue<'_, Read<T>, F, B>
where
    E: Entity,
    T: 'static + Sync,
    F: Query,
    B: BorrowState,
{
    type Output = T;

    #[inline(always)]
    fn index(&self, entity: E) -> &T {
        let entity = entity
            .entity_loc(self.entity_set)
            .expect("Entity is not alive");
        self.get(entity)
            .expect("Entity does not match view's query and filter")
    }
}

impl<E, T, F, B> Index<E> for ViewValue<'_, Write<T>, F, B>
where
    E: Entity,
    T: 'static + Send,
    F: Query,
    B: BorrowState,
{
    type Output = T;

    #[inline(always)]
    fn index(&self, entity: E) -> &T {
        let entity = entity
            .entity_loc(self.entity_set)
            .expect("Entity is not alive");
        self.get(entity)
            .expect("Entity does not match view's query and filter")
    }
}

impl<E, T, F, B> IndexMut<E> for ViewValue<'_, Write<T>, F, B>
where
    E: Entity,
    T: 'static + Send,
    F: Query,
    B: BorrowState,
{
    #[inline(always)]
    fn index_mut(&mut self, entity: E) -> &mut T {
        let entity = entity
            .entity_loc(self.entity_set)
            .expect("Entity is not alive");
        self.get(entity)
            .expect("Entity does not match view's query and filter")
    }
}
