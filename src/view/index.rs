use core::ops::{Index, IndexMut};

use crate::{
    archetype::chunk_idx,
    entity::{AliveEntity, Entity, Location},
    query::{Fetch, IntoQuery, Query},
};

use super::{BorrowState, View};

#[inline(always)]
#[track_caller]
fn expect_match<T>(value: Option<T>) -> T {
    value.expect("Entity does not match view's query and filter")
}

#[inline(always)]
#[track_caller]
fn expect_alive<T>(value: Option<T>) -> T {
    value.expect("Entity is not alive")
}

impl<Q, F, B> View<'_, Q, F, B>
where
    Q: IntoQuery,
    F: IntoQuery,
    B: BorrowState,
{
    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline(always)]
    pub fn get_entity(&self, entity: impl AliveEntity) -> Option<Q::Query::Item<'_>> {
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
    pub fn get_entity_expect(&self, entity: impl Entity) -> Q::Query::Item<'_> {
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
    pub fn with_entity<Fun, R>(&self, entity: impl AliveEntity, f: Fun) -> R
    where
        Fun: FnOnce(Option<Q::Query::Item<'_>>) -> R,
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

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Otherwise, calls closure with `None`.
    #[inline(always)]
    #[track_caller]
    pub fn with_entity_expect<Fun, R>(&self, entity: impl Entity, f: Fun) -> R
    where
        Fun: FnOnce(Q::Query::Item<'_>) -> R,
    {
        let entity = expect_alive(entity.lookup(self.entity_set));
        let Location { arch, idx } = entity.location();

        if arch == u32::MAX {
            return f(expect_match(Query::reserved_entity_item(
                &self.query,
                entity.id(),
                idx,
            )));
        }

        let archetype = &self.archetypes[arch];

        // Ensure to borrow view's data.
        self.borrow.with(&self.query, &self.filter, archetype, || {
            f(expect_match(self._get(Location { arch, idx })))
        })
    }

    #[inline]
    fn _get(&self, location: Location) -> Option<Q::Query::Item<'_>> {
        let Location { arch, idx } = location;
        debug_assert_ne!(arch, u32::MAX);

        let archetype = &self.archetypes[arch as usize];
        assert!(idx < archetype.len(), "Wrong location");

        if !unsafe { Query::visit_archetype(&self.query, archetype) } {
            return None;
        }

        if !unsafe { Query::visit_archetype(&self.filter, archetype) } {
            return None;
        }

        let epoch = self.epochs.next_if(Q::Query::MUTABLE || F::Query::MUTABLE);

        let mut query_fetch = unsafe { Query::fetch(&self.query, arch, archetype, epoch) };

        if !unsafe { Fetch::visit_chunk(&mut query_fetch, chunk_idx(idx)) } {
            return None;
        }

        unsafe { Fetch::touch_chunk(&mut query_fetch, chunk_idx(idx)) }

        if !unsafe { Fetch::visit_item(&mut query_fetch, idx) } {
            return None;
        }

        let mut filter_fetch = unsafe { Query::fetch(&self.filter, arch, archetype, epoch) };

        if !unsafe { Fetch::visit_chunk(&mut filter_fetch, chunk_idx(idx)) } {
            return None;
        }

        unsafe { Fetch::touch_chunk(&mut filter_fetch, chunk_idx(idx)) }

        if !unsafe { Fetch::visit_item(&mut filter_fetch, idx) } {
            return None;
        }

        Some(unsafe { Fetch::get_item(&mut query_fetch, idx) })
    }
}

impl<E, T, F, B> Index<E> for View<'_, &T, F, B>
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

impl<E, T, F, B> Index<E> for View<'_, &mut T, F, B>
where
    E: Entity,
    T: 'static + Send,
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

impl<E, T, F, B> IndexMut<E> for View<'_, &mut T, F, B>
where
    E: Entity,
    T: 'static + Send,
    F: IntoQuery,
    B: BorrowState,
{
    #[inline(always)]
    fn index_mut(&mut self, entity: E) -> &mut T {
        let entity = entity.lookup(self.entity_set).expect("Entity is not alive");
        self.get(entity)
            .expect("Entity does not match view's query and filter")
    }
}
