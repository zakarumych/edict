//! Methods for fetching components from specific entities.

use crate::{
    archetype::chunk_idx,
    entity::{AliveEntity, Entity},
    query::{DefaultQuery, DefaultSendQuery, Fetch, IntoQuery, IntoSendQuery, Query, QueryItem},
    view::{ViewOne, ViewOneValue},
    EntityError, NoSuchEntity,
};

use super::{World, WorldLocal};

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
    #[inline]
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
    #[inline]
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
                .ok_or(EntityError::Mismatch);
        }

        let archetype = &self.archetypes[loc.arch as usize];

        debug_assert!(archetype.len() >= loc.idx, "Entity index is valid");

        if !query.visit_archetype(archetype) {
            return Err(EntityError::Mismatch);
        }

        let epoch = self.epoch.next();

        let mut fetch = unsafe { query.fetch(loc.arch, archetype, epoch) };

        if !unsafe { fetch.visit_chunk(chunk_idx(loc.idx)) } {
            return Err(EntityError::Mismatch);
        }

        unsafe { fetch.touch_chunk(chunk_idx(loc.idx)) };

        if !unsafe { fetch.visit_item(loc.idx) } {
            return Err(EntityError::Mismatch);
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
    #[inline]
    pub fn view_one<'a, Q>(&'a self, entity: impl AliveEntity) -> ViewOne<'a, Q>
    where
        Q: DefaultSendQuery,
    {
        ViewOneValue::new(self, entity, Q::default_query(), ())
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
    #[inline]
    pub fn try_view_one<'a, Q>(
        &'a self,
        entity: impl Entity,
    ) -> Result<ViewOne<'a, Q>, NoSuchEntity>
    where
        Q: DefaultSendQuery,
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
    #[inline]
    pub fn view_one_with<'a, Q>(&'a self, entity: impl AliveEntity, query: Q) -> ViewOne<'a, (Q,)>
    where
        Q: IntoSendQuery,
    {
        ViewOneValue::new(self, entity, (query.into_query(),), ())
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
    #[inline]
    pub fn try_view_one_with<'a, Q>(
        &'a self,
        entity: impl Entity,
        query: Q,
    ) -> Result<ViewOne<'a, (Q,)>, NoSuchEntity>
    where
        Q: IntoSendQuery,
    {
        let entity = self.lookup(entity)?;
        Ok(self.view_one_with::<Q>(entity, query))
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    #[inline]
    pub fn get_cloned<T>(&mut self, entity: impl AliveEntity) -> Option<T>
    where
        T: Clone + 'static,
    {
        match self.get::<&T>(entity) {
            Ok(item) => Some(item.clone()),
            Err(_) => None,
        }
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    #[inline]
    pub fn try_get_cloned<T>(&mut self, entity: impl Entity) -> Result<T, EntityError>
    where
        T: Clone + 'static,
    {
        match self.get::<&T>(entity) {
            Ok(item) => Ok(item.clone()),
            Err(err) => Err(err),
        }
    }
}

impl WorldLocal {
    /// Queries components from specified entity.
    ///
    /// Returns a wrapper from which query item can be fetched.
    ///
    /// The wrapper holds borrow locks for entity's archetype and releases them on drop.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline]
    pub fn view_one<'a, Q>(&'a self, entity: impl AliveEntity) -> ViewOne<'a, Q>
    where
        Q: DefaultQuery,
    {
        ViewOneValue::new(self, entity, Q::default_query(), ())
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
    #[inline]
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
    #[inline]
    pub fn view_one_with<'a, Q>(&'a self, entity: impl AliveEntity, query: Q) -> ViewOne<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewOneValue::new(self, entity, (query.into_query(),), ())
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
    #[inline]
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
}
