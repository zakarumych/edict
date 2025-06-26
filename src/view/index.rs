use core::ops::Index;

use crate::{
    entity::{AliveEntity, Entity, EntityId, Location},
    query::{ImmutableQuery, Query, QueryItem, Read},
    view::get_at,
    EntityError, NoSuchEntity,
};

use super::{expect_alive, expect_match, BorrowState, ViewValue};

impl<Q, F, B, E> ViewValue<'_, Q, F, B, E>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline]
    pub fn get_mut(&mut self, entity: impl AliveEntity) -> Option<QueryItem<'_, Q>> {
        let loc = entity.locate(self.entity_set);

        if loc.arch == u32::MAX {
            return self._get_reserved(entity.id(), loc);
        }

        // Ensure to borrow view's data.
        self.acquire_borrow();

        unsafe { self._get(loc) }
    }

    /// Fetches data that matches the view's query and filter
    /// from a single entity.
    ///
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    /// Returns `Ok(None)` if entity does not match the view's query and filter.
    #[inline]
    pub fn try_get_mut(&mut self, entity: impl Entity) -> Result<QueryItem<'_, Q>, EntityError> {
        let loc = entity.lookup(self.entity_set).ok_or(NoSuchEntity)?;

        if loc.arch == u32::MAX {
            return self
                ._get_reserved(entity.id(), loc)
                .ok_or(EntityError::Mismatch);
        }

        // Ensure to borrow view's data.
        self.acquire_borrow();

        unsafe { self._get(loc) }.ok_or(EntityError::Mismatch)
    }

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline]
    #[track_caller]
    pub fn expect_mut(&mut self, entity: impl Entity) -> QueryItem<'_, Q> {
        let loc = expect_alive(entity.lookup(self.entity_set));

        if loc.arch == u32::MAX {
            return expect_match(self._get_reserved(entity.id(), loc));
        }

        // Ensure to borrow view's data.
        self.acquire_borrow();

        expect_match(unsafe { self._get(loc) })
    }

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Returns result of the closure or `None` if entity does not match
    /// query and filter.
    #[inline]
    pub fn map_mut<Fun, R>(&mut self, entity: impl AliveEntity, f: Fun) -> Option<R>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        let loc = entity.locate(self.entity_set);

        if loc.arch == u32::MAX {
            return self._get_reserved(entity.id(), loc).map(f);
        }

        // Ensure to borrow view's data.
        self.with_borrow(loc.arch, || unsafe { self._get(loc) }.map(f))
    }

    /// Fetches data that matches the view's query and filter
    /// from a single entity.
    ///
    /// Calls provided closure with fetched data if entity is alive and matches query and filter.
    /// Returns result of the closure or `Err(NoSuchEntity)` if entity is not alive
    /// or `Ok(None)` if entity is alive but does not match query and filter.
    #[inline]
    pub fn try_map_mut<Fun, R>(&mut self, entity: impl Entity, f: Fun) -> Result<R, EntityError>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        let loc = entity
            .lookup(self.entity_set)
            .ok_or(EntityError::NoSuchEntity)?;

        if loc.arch == u32::MAX {
            return self
                ._get_reserved(entity.id(), loc)
                .map(f)
                .ok_or(EntityError::Mismatch);
        }

        // Ensure to borrow view's data.
        self.with_borrow(loc.arch, || unsafe { self._get(loc) }.map(f))
            .ok_or(EntityError::Mismatch)
    }

    #[inline]
    fn _get_reserved(&self, id: EntityId, loc: Location) -> Option<QueryItem<'_, Q>> {
        debug_assert_eq!(loc.arch, u32::MAX);

        Query::reserved_entity_item(&self.query, id, loc.idx)
    }

    #[inline]
    unsafe fn _get(&self, loc: Location) -> Option<QueryItem<'_, Q>> {
        debug_assert_ne!(loc.arch, u32::MAX);

        unsafe {
            get_at(
                self.query,
                self.filter,
                self.epochs,
                &self.archetypes[loc.arch as usize],
                loc,
            )
        }
    }
}

impl<Q, F, B, E> ViewValue<'_, Q, F, B, E>
where
    Q: ImmutableQuery,
    F: ImmutableQuery,
    B: BorrowState,
{
    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline]
    pub fn get(&self, entity: impl AliveEntity) -> Option<QueryItem<'_, Q>> {
        let loc = entity.locate(self.entity_set);

        if loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, entity.id(), loc.idx);
        }

        // Ensure to borrow view's data.
        self.acquire_borrow();

        unsafe { self._get(loc) }
    }

    /// Fetches data that matches the view's query and filter
    /// from a single entity.
    ///
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    /// Returns `Ok(None)` if entity does not match the view's query and filter.
    #[inline]
    pub fn try_get(&self, entity: impl Entity) -> Result<QueryItem<'_, Q>, EntityError> {
        let loc = entity.lookup(self.entity_set).ok_or(NoSuchEntity)?;

        if loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, entity.id(), loc.idx)
                .ok_or(EntityError::Mismatch);
        }

        // Ensure to borrow view's data.
        self.acquire_borrow();

        unsafe { self._get(loc) }.ok_or(EntityError::Mismatch)
    }

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Returns none if entity does not match the view's query and filter.
    #[inline]
    #[track_caller]
    pub fn expect(&self, entity: impl Entity) -> QueryItem<'_, Q> {
        let loc = expect_alive(entity.lookup(self.entity_set));

        if loc.arch == u32::MAX {
            return expect_match(Query::reserved_entity_item(
                &self.query,
                entity.id(),
                loc.idx,
            ));
        }

        // Ensure to borrow view's data.
        self.acquire_borrow();

        expect_match(unsafe { self._get(loc) })
    }

    /// Fetches data that matches the view's query and filter
    /// from a single alive entity.
    ///
    /// Calls provided closure with fetched data if entity matches query and filter.
    /// Returns result of the closure or `None` if entity does not match
    /// query and filter.
    #[inline]
    pub fn map<Fun, R>(&self, entity: impl AliveEntity, f: Fun) -> Option<R>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        let loc = entity.locate(self.entity_set);

        if loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, entity.id(), loc.idx).map(f);
        }

        // Ensure to borrow view's data.
        self.with_borrow(loc.arch, || unsafe { self._get(loc) }.map(f))
    }

    /// Fetches data that matches the view's query and filter
    /// from a single entity.
    ///
    /// Calls provided closure with fetched data if entity is alive and matches query and filter.
    /// Returns result of the closure or `Err(NoSuchEntity)` if entity is not alive
    /// or `Ok(None)` if entity is alive but does not match query and filter.
    #[inline]
    pub fn try_map<Fun, R>(&self, entity: impl Entity, f: Fun) -> Result<R, EntityError>
    where
        Fun: FnOnce(QueryItem<Q>) -> R,
    {
        let loc = entity
            .lookup(self.entity_set)
            .ok_or(EntityError::NoSuchEntity)?;

        if loc.arch == u32::MAX {
            return Query::reserved_entity_item(&self.query, entity.id(), loc.idx)
                .map(f)
                .ok_or(EntityError::Mismatch);
        }

        // Ensure to borrow view's data.
        self.with_borrow(loc.arch, || unsafe { self._get(loc) }.map(f))
            .ok_or(EntityError::Mismatch)
    }
}

impl<E, T, F, B, X> Index<E> for ViewValue<'_, Read<T>, F, B, X>
where
    E: Entity,
    T: 'static + Sync,
    F: ImmutableQuery,
    B: BorrowState,
{
    type Output = T;

    #[inline]
    fn index(&self, entity: E) -> &T {
        let entity = entity
            .entity_loc(self.entity_set)
            .expect("Entity is not alive");
        self.get(entity)
            .expect("Entity does not match view's query and filter")
    }
}

// impl<E, T, F, B> Index<E> for ViewValue<'_, Write<T>, F, B>
// where
//     E: Entity,
//     T: 'static + Send,
//     F: ImmutableQuery,
//     B: BorrowState,
// {
//     type Output = T;

//     #[inline]
//     fn index(&self, entity: E) -> &T {
//         let entity = entity
//             .entity_loc(self.entity_set)
//             .expect("Entity is not alive");
//         unsafe { self._get(entity.location()) }
//             .expect("Entity does not match view's query and filter")
//     }
// }

// impl<E, T, F, B> IndexMut<E> for ViewValue<'_, Write<T>, F, B>
// where
//     E: Entity,
//     T: 'static + Send,
//     F: ImmutableQuery,
//     B: BorrowState,
// {
//     #[inline]
//     fn index_mut(&mut self, entity: E) -> &mut T {
//         let entity = entity
//             .entity_loc(self.entity_set)
//             .expect("Entity is not alive");
//         self.get(entity)
//             .expect("Entity does not match view's query and filter")
//     }
// }
