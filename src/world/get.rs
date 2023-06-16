//! Methods for fetching components from specific entities.

use crate::{
    archetype::chunk_idx,
    entity::AliveEntity,
    query::{DefaultQuery, Fetch, IntoQuery, Query, QueryItem},
    view::{ViewOne, ViewOneState},
};

use super::World;

impl World {
    /// Queries components from specified entity.
    ///
    /// Returns query item.
    ///
    /// This method works only for default-constructed query types.
    ///
    /// Mutably borrows world for the duration of query item's lifetime,
    /// avoiding runtime borrow checks.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn get_mut<'a, Q>(&'a mut self, entity: impl AliveEntity) -> Option<QueryItem<'a, Q>>
    where
        Q: DefaultQuery,
    {
        self.get_with_mut(entity, Q::default_query())
    }

    /// Queries components from specified entity.
    ///
    /// Returns query item.
    ///
    /// Mutably borrows world for the duration of query item's lifetime,
    /// avoiding runtime borrow checks.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn get_with_mut<'a, Q>(
        &'a mut self,
        entity: impl AliveEntity,
        query: Q,
    ) -> Option<QueryItem<'a, Q::Query>>
    where
        Q: IntoQuery,
    {
        unsafe { self.get_with_unchecked::<Q>(entity, query) }
    }

    /// Queries components from specified entity.
    ///
    /// Returns query item.
    ///
    /// This method works only for default-constructed query types.
    ///
    /// # Safety
    ///
    /// Caller must guarantee to not create invalid aliasing of component
    /// references.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub unsafe fn get_unchecked<'a, Q>(
        &'a self,
        entity: impl AliveEntity,
    ) -> Option<QueryItem<'a, Q>>
    where
        Q: DefaultQuery,
    {
        unsafe { self.get_with_unchecked(entity, Q::default_query()) }
    }

    /// Queries components from specified entity.
    ///
    /// Returns query item.
    ///
    /// # Safety
    ///
    /// Caller must guarantee to not create invalid aliasing of component
    /// references.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline]
    pub unsafe fn get_with_unchecked<'a, Q>(
        &'a self,
        entity: impl AliveEntity,
        query: Q,
    ) -> Option<QueryItem<'a, Q::Query>>
    where
        Q: IntoQuery,
    {
        let mut query = query.into_query();

        let loc = entity.locate(&self.entities);

        if loc.arch == u32::MAX {
            // Reserved entity
            return query.reserved_entity_item(entity.id(), loc.idx);
        }

        let archetype = &self.archetypes[loc.arch as usize];

        debug_assert!(archetype.len() >= loc.idx as usize, "Entity index is valid");

        if !query.visit_archetype(archetype) {
            return None;
        }

        let epoch = self.epoch.next();

        let mut fetch = unsafe { query.fetch(loc.arch, archetype, epoch) };

        if !unsafe { fetch.visit_chunk(chunk_idx(loc.idx)) } {
            return None;
        }

        unsafe { fetch.touch_chunk(chunk_idx(loc.idx)) };

        if !unsafe { fetch.visit_item(loc.idx) } {
            return None;
        }

        let item = unsafe { fetch.get_item(loc.idx) };

        Some(item)
    }

    /// Queries components from specified entity.
    ///
    /// Returns a wrapper from which query item can be fetched.
    ///
    /// The wrapper holds borrow locks for entity's archetype and releases them on drop.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn view_one<'a, Q>(&'a self, entity: impl AliveEntity) -> ViewOne<'a, Q>
    where
        Q: DefaultQuery,
    {
        ViewOneState::new(self, entity)
    }

    /// Queries components from specified entity.
    /// This method accepts query instance to support stateful queries.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline(always)]
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    pub fn view_one_with<'a, Q>(&'a self, entity: impl AliveEntity, query: Q) -> ViewOne<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewOneState::with_query(self, entity, query)
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_cloned<T>(&mut self, entity: impl AliveEntity) -> Option<T>
    where
        T: Clone + Sync + 'static,
    {
        self.view_one::<&T>(entity).map(Clone::clone)
    }
}
