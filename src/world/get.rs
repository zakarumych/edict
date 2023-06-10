//! Methods for fetching components from specific entities.

use crate::{
    archetype::chunk_idx,
    entity::{AliveEntity, Entity},
    query::{DefaultQuery, IntoQuery, QueryItem},
};

use super::{QueryOneError, World};

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
    pub fn get_mut<'a, Q>(&'a mut self, entity: impl AliveEntity) -> Option<QueryItem<'a, Q::Query>>
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
    ) -> Result<QueryItem<'a, Q::Query>, QueryOneError>
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
    ) -> Result<QueryItem<'a, Q>, QueryOneError>
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

        let mut fetch = unsafe { query.fetch(archetype, epoch) };

        if !unsafe { fetch.visit_chunk(chunk_idx(loc.idx as usize)) } {
            return None;
        }

        unsafe { fetch.touch_chunk(chunk_idx(loc.idx as usize)) };

        if !unsafe { fetch.visit_item(loc.idx as usize) } {
            return None;
        }

        let item = unsafe { fetch.get_item(loc.idx as usize) };

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
    pub fn get<'a, Q>(&'a self, entity: impl AliveEntity) -> ViewOne<'a, Q>
    where
        Q: DefaultQuery,
    {
        let query = Q::default_query();

        let loc = entity.locate(&self.entities);

        let query = query.into_query();
        if loc.arch == u32::MAX {
            return Ok(ViewOne::new_reserved(query, entity.id(), &self.epoch));
        }

        let archetype = &self.archetypes[loc.arch as usize];

        debug_assert!(archetype.len() >= loc.idx as usize, "Entity index is valid");

        Ok(ViewOne::new(query, archetype, loc.idx, &self.epoch))
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
    pub fn get_with<'a, Q>(&'a self, entity: impl Entity, query: Q) -> ViewOne<'a, Q>
    where
        Q: IntoQuery,
    {
        let query = query.into_query();
        self._get(entity, query)
    }

    /// Implementation of `get` and `get_with` methods.
    #[inline]
    fn _get<'a, Q>(&'a self, entity: impl Entity, query: Q::Query) -> ViewOne<'a, Q>
    where
        Q: IntoQuery,
    {
        let loc = entity.locate(&self.entities);

        let query = query.into_query();
        if loc.arch == u32::MAX {
            return Ok(ViewOne::new_reserved(query, entity.id(), &self.epoch));
        }

        let archetype = &self.archetypes[loc.arch as usize];

        debug_assert!(archetype.len() >= loc.idx as usize, "Entity index is valid");

        Ok(ViewOne::new(query, archetype, loc.idx, &self.epoch))
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_owned<Q, T>(&mut self, id: EntityId) -> Result<T::Owned, QueryOneError>
    where
        T: ToOwned + 'static,
        Q: DefaultQuery,
        Q::Query: for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one::<Q, _, _>(id, |item| T::to_owned(item))
    }

    /// Where query item is a reference to value the implements [`Clone`].
    /// Returns cloned item value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_one_cloned<Q, T>(&mut self, id: EntityId) -> Result<T, QueryOneError>
    where
        T: Clone + 'static,
        Q: DefaultQuery,
        Q::Query: for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one::<Q, _, _>(id, |item| T::clone(item))
    }
    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`Copy`].
    /// Returns copied item value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_one_copied<Q, T>(&mut self, id: EntityId) -> Result<T, QueryOneError>
    where
        T: Copy + 'static,
        Q: DefaultQuery,
        Q::Query: for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one::<Q, _, _>(id, |item| *item)
    }
}
