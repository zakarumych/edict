use core::ops::{Index, IndexMut};

use crate::{
    entity::{AliveEntity, Entity, Location},
    query::{IntoQuery, Query, QueryItem, Read, Write},
    view::get_at,
};

use super::{expect_alive, expect_match, BorrowState, ViewState};

impl<Q, F, B> ViewState<'_, Q, F, B>
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
        let entity = entity.locate(self.entity_set);
        let Location { arch, idx } = entity.location();

        if arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, entity.id(), idx);
        }

        // Ensure to borrow view's data.
        self.borrow
            .acquire(&self.query, &self.filter, self.archetypes);

        self._get(Location { arch, idx })
    }

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline(always)]
    #[track_caller]
    pub fn expect(&self, entity: impl Entity) -> QueryItem<Q> {
        let entity = expect_alive(entity.lookup(self.entity_set));
        let Location { arch, idx } = entity.location();

        if arch == u32::MAX {
            return expect_match(Query::reserved_entity_item(&self.query, entity.id(), idx));
        }

        // Ensure to borrow view's data.
        self.borrow
            .acquire(&self.query, &self.filter, self.archetypes);

        expect_match(self._get(Location { arch, idx }))
    }

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Otherwise, calls closure with `None`.
    #[inline(always)]
    pub fn with<Fun, R>(&self, entity: impl AliveEntity, f: Fun) -> R
    where
        Fun: FnOnce(Option<QueryItem<Q>>) -> R,
    {
        let entity = entity.locate(self.entity_set);
        let Location { arch, idx } = entity.location();

        if arch == u32::MAX {
            return f(Query::reserved_entity_item(&self.query, entity.id(), idx));
        }

        let archetype = &self.archetypes[arch];

        // Ensure to borrow view's data.
        self.borrow.with(&self.query, &self.filter, archetype, || {
            f(self._get(Location { arch, idx }))
        })
    }

    #[inline]
    fn _get(&self, loc: Location) -> Option<QueryItem<Q>> {
        debug_assert_ne!(loc.arch, u32::MAX);

        let archetype = &self.archetypes[loc.arch as usize];
        get_at(&self.query, &self.filter, self.epochs, archetype, loc)
    }
}

impl<E, T, F, B> Index<E> for ViewState<'_, Read<T>, F, B>
where
    E: Entity,
    T: 'static + Sync,
    F: IntoQuery,
    B: BorrowState,
{
    type Output = T;

    #[inline(always)]
    fn index(&self, entity: E) -> &T {
        let entity = entity.lookup(self.entity_set).expect("Entity is not alive");
        self.get(entity)
            .expect("Entity does not match view's query and filter")
    }
}

impl<E, T, F, B> Index<E> for ViewState<'_, Write<T>, F, B>
where
    E: Entity,
    T: 'static + Send,
    F: Query,
    B: BorrowState,
{
    type Output = T;

    #[inline(always)]
    fn index(&self, entity: E) -> &T {
        let entity = entity.lookup(self.entity_set).expect("Entity is not alive");
        self.get(entity)
            .expect("Entity does not match view's query and filter")
    }
}

impl<E, T, F, B> IndexMut<E> for ViewState<'_, Write<T>, F, B>
where
    E: Entity,
    T: 'static + Send,
    F: Query,
    B: BorrowState,
{
    #[inline(always)]
    fn index_mut(&mut self, entity: E) -> &mut T {
        let entity = entity.lookup(self.entity_set).expect("Entity is not alive");
        self.get(entity)
            .expect("Entity does not match view's query and filter")
    }
}
