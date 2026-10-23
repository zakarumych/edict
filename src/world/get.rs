//! Methods for fetching components from specific entities.

use crate::{
    EntityError, NoSuchEntity,
    archetype::{Archetype, chunk_idx},
    entity::{AliveEntity, Entity, EntitySet},
    epoch::{EpochId, EpochSource},
    query::{
        AsDefaultQuery, AsDefaultSendQuery, Fetch, IntoQuery, IntoSendQuery, Query, QueryCacheRef,
        QueryItem,
    },
    view::{LocalViewOne, ViewOne, ViewOneValue},
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
        Q: AsDefaultQuery,
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
        let query = self.cache.get(query.into_query());
        // Safety: exclusive World access prevents conflicting component references.
        // EpochCounter is borrowed separately from the entity and component storage.
        unsafe {
            get_with_cached(query, &self.entities, &self.archetypes, entity, || {
                self.epoch.next_mut()
            })
        }
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
        Q: AsDefaultQuery,
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
        let query = self.cache.get(query.into_query());
        // Safety: the caller guarantees that component references do not conflict.
        unsafe {
            get_with_cached(query, &self.entities, &self.archetypes, entity, || {
                self.epoch.next()
            })
        }
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
        Q: AsDefaultSendQuery,
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
        Q: AsDefaultSendQuery,
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
    /// Queries a local entity without runtime component borrow checks.
    ///
    /// # Safety
    /// The query must not create conflicting component references.
    #[inline]
    pub unsafe fn get_unchecked<'a, Q: AsDefaultQuery>(
        &'a self,
        entity: impl Entity,
    ) -> Result<QueryItem<'a, Q>, EntityError> {
        unsafe { self.get_with_unchecked(entity, Q::default_query()) }
    }

    /// Queries a local entity with a provided query and local epoch allocation.
    ///
    /// # Safety
    /// The query must not create conflicting component references.
    #[inline]
    pub unsafe fn get_with_unchecked<'a, Q: IntoQuery>(
        &'a self,
        entity: impl Entity,
        query: Q,
    ) -> Result<QueryItem<'a, Q::Query>, EntityError> {
        let query = self.inner.cache.get(query.into_query());
        // Safety: the caller excludes component aliases, and the local borrow
        // excludes atomic-world access for the lifetime of the returned item.
        unsafe {
            get_with_cached(
                query,
                &self.inner.entities,
                &self.inner.archetypes,
                entity,
                || self.local_epochs().next(),
            )
        }
    }

    /// Creates a local single-entity view with runtime component borrow checks.
    #[inline]
    pub fn view_one<'a, Q: AsDefaultQuery>(
        &'a self,
        entity: impl AliveEntity,
    ) -> LocalViewOne<'a, Q> {
        // Safety: the local borrow excludes shared-world epoch access.
        unsafe {
            ViewOneValue::new_with_epochs(
                &self.inner,
                entity,
                Q::default_query(),
                (),
                self.local_epochs(),
            )
        }
    }

    /// Creates a local single-entity view if the entity exists.
    #[inline]
    pub fn try_view_one<'a, Q: AsDefaultQuery>(
        &'a self,
        entity: impl Entity,
    ) -> Result<LocalViewOne<'a, Q>, NoSuchEntity> {
        let entity = self.lookup(entity)?;
        Ok(self.view_one::<Q>(entity))
    }

    /// Creates a local single-entity view with a provided query.
    #[inline]
    pub fn view_one_with<'a, Q: IntoQuery>(
        &'a self,
        entity: impl AliveEntity,
        query: Q,
    ) -> LocalViewOne<'a, (Q,)> {
        // Safety: the local borrow excludes shared-world epoch access.
        unsafe {
            ViewOneValue::new_with_epochs(
                &self.inner,
                entity,
                (query.into_query(),),
                (),
                self.local_epochs(),
            )
        }
    }

    /// Creates a local single-entity view with a provided query if the entity exists.
    #[inline]
    pub fn try_view_one_with<'a, Q: IntoQuery>(
        &'a self,
        entity: impl Entity,
        query: Q,
    ) -> Result<LocalViewOne<'a, (Q,)>, NoSuchEntity> {
        let entity = self.lookup(entity)?;
        Ok(self.view_one_with::<Q>(entity, query))
    }
}

/// Fetches one entity, reserving an epoch only after entity and archetype matching.
///
/// # Safety
/// Component references produced by the query must not alias conflicting references.
#[inline]
unsafe fn get_with_cached<'a, Q: Query>(
    query: QueryCacheRef<'a, Q>,
    entities: &'a EntitySet,
    archetypes: &'a [Archetype],
    entity: impl Entity,
    next_epoch: impl FnOnce() -> EpochId,
) -> Result<Q::Item<'a>, EntityError> {
    let loc = entity.lookup(entities).ok_or(EntityError::NoSuchEntity)?;

    if loc.arch == u32::MAX {
        return query
            .reserved_entity_item(entity.id(), loc.idx)
            .ok_or(EntityError::Mismatch);
    }

    let archetype = &archetypes[loc.arch as usize];
    assert!(loc.idx < archetype.len(), "Entity index is valid");

    if !query.visit_archetype(archetype, loc.arch) {
        return Err(EntityError::Mismatch);
    }
    if !unsafe { query.visit_archetype_late(archetype) } {
        return Err(EntityError::Mismatch);
    }

    let epoch = next_epoch();
    let mut fetch = unsafe { query.fetch(loc.arch, archetype, epoch) };
    if !unsafe { fetch.visit_chunk(chunk_idx(loc.idx)) } {
        return Err(EntityError::Mismatch);
    }
    unsafe { fetch.touch_chunk(chunk_idx(loc.idx)) };
    if !unsafe { fetch.visit_item(loc.idx) } {
        return Err(EntityError::Mismatch);
    }
    Ok(unsafe { fetch.get_item(loc.idx) })
}
