//! Methods for fetching components from specific entities.

use crate::{
    archetype::chunk_idx,
    entity::{AliveEntity, Entity},
    query::{DefaultQuery, Fetch, IntoQuery, Query, QueryItem},
    view::{ViewOne, ViewOneState},
    EntityError, NoSuchEntity,
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
    pub fn get<'a, Q>(&'a mut self, entity: impl Entity) -> Result<QueryItem<'a, Q>, EntityError>
    where
        Q: DefaultQuery,
    {
        self.get_with(entity, Q::default_query())
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
    pub fn get_with<'a, Q>(
        &'a mut self,
        entity: impl Entity,
        query: Q,
    ) -> Result<QueryItem<'a, Q::Query>, EntityError>
    where
        Q: IntoQuery,
    {
        unsafe { self.get_with_unchecked::<Q>(entity, query) }
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
    pub unsafe fn get_unchecked<'a, Q>(
        &'a self,
        entity: impl Entity,
    ) -> Result<QueryItem<'a, Q::Query>, EntityError>
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
        entity: impl Entity,
        query: Q,
    ) -> Result<QueryItem<'a, Q::Query>, EntityError>
    where
        Q: IntoQuery,
    {
        let query = query.into_query();

        let loc = entity
            .lookup(&self.entities)
            .ok_or(EntityError::NoSuchEntity)?;

        if loc.arch == u32::MAX {
            // Reserved entity
            return query
                .reserved_entity_item(entity.id(), loc.idx)
                .ok_or(EntityError::QueryMismatch);
        }

        let archetype = &self.archetypes[loc.arch as usize];

        debug_assert!(archetype.len() >= loc.idx as usize, "Entity index is valid");

        if !query.visit_archetype(archetype) {
            return Err(EntityError::QueryMismatch);
        }

        let epoch = self.epoch.next();

        let mut fetch = unsafe { query.fetch(loc.arch, archetype, epoch) };

        if !unsafe { fetch.visit_chunk(chunk_idx(loc.idx)) } {
            return Err(EntityError::QueryMismatch);
        }

        unsafe { fetch.touch_chunk(chunk_idx(loc.idx)) };

        if !unsafe { fetch.visit_item(loc.idx) } {
            return Err(EntityError::QueryMismatch);
        }

        let item = unsafe { fetch.get_item(loc.idx) };

        Ok(item)
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
        ViewOneState::new(self, entity, Q::default_query(), ())
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
    pub fn try_view_one<'a, Q>(
        &'a self,
        entity: impl Entity,
    ) -> Result<ViewOne<'a, Q>, NoSuchEntity>
    where
        Q: DefaultQuery,
    {
        let entity = self.lookup(entity)?;
        Ok(self.view_one::<Q>(entity))
    }

    /// Queries components from specified entity.
    /// This method accepts query instance to support stateful queries.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn view_one_with<'a, Q>(&'a self, entity: impl AliveEntity, query: Q) -> ViewOne<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewOneState::new(self, entity, (query.into_query(),), ())
    }

    /// Queries components from specified entity.
    /// This method accepts query instance to support stateful queries.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn try_view_one_with<'a, Q>(
        &'a self,
        entity: impl Entity,
        query: Q,
    ) -> Result<ViewOne<'a, (Q,)>, NoSuchEntity>
    where
        Q: IntoQuery,
    {
        let entity = self.lookup(entity)?;
        Ok(self.view_one_with::<Q>(entity, query))
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    #[inline(always)]
    pub fn get_cloned<T>(&self, entity: impl AliveEntity) -> Option<T>
    where
        T: Clone + Sync + 'static,
    {
        self.view_one::<&T>(entity).map(Clone::clone)
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    #[inline(always)]
    pub fn try_get_cloned<T>(&self, entity: impl Entity) -> Result<Option<T>, NoSuchEntity>
    where
        T: Clone + Sync + 'static,
    {
        let entity = self.lookup(entity)?;
        Ok(self.get_cloned::<T>(entity))
    }
}
