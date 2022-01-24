//! Self-contained ECS [`World`].

use core::{
    any::{type_name, TypeId},
    fmt,
    hash::{BuildHasher, Hash, Hasher},
    iter::FromIterator,
    iter::FusedIterator,
    marker::PhantomData,
};

use alloc::vec::Vec;
use hashbrown::{
    hash_map::{Entry, RawEntryMut},
    HashMap,
};

use crate::{
    archetype::{Archetype, CHUNK_LEN_USIZE},
    bundle::{Bundle, DynamicBundle},
    component::{Component, ComponentInfo},
    entity::{Entities, EntityId},
    hash::{MulHasherBuilder, NoOpHasherBuilder},
    idx::MAX_IDX_USIZE,
    query::{
        Fetch, ImmutableQuery, NonTrackingQuery, Query, QueryItem, QueryIter, QueryTrackedIter,
    },
};
#[cfg(feature = "rc")]
use crate::{entity::Entity, proof::Proof};

/// Value to remember which modifications was already iterated over,
/// and see what modifications are new.
#[derive(Clone, Debug)]
#[allow(missing_copy_implementations)]
pub struct Tracks {
    pub(crate) epoch: u64,
}

/// Limits on reserving of space for entities and components
/// in archetypes when `spawn_batch` is used.
const MAX_SPAWN_RESERVE: usize = 1024;

fn spawn_reserve(iter: &impl Iterator, archetype: &mut Archetype) {
    let (lower, upper) = iter.size_hint();
    let additional = match (lower, upper) {
        (lower, None) => lower,
        (lower, Some(upper)) => {
            // Iterator is consumed in full, so reserve at least `lower`.
            lower.max(upper.min(MAX_SPAWN_RESERVE))
        }
    };
    archetype.reserve(additional);
}

#[derive(Clone, Debug)]
struct InsertInfo {
    // meta: InsertMeta,
    dst: u32,
}

#[derive(Clone, Debug)]
struct InsertBundleInfo {
    // meta: InsertBundleMeta,
    dst: u32,
}

#[derive(Clone, Debug)]
struct RemoveInfo {
    // meta: RemoveMeta,
    dst: u32,
}

#[derive(Clone, Debug)]
struct RemoveBundleInfo {
    // meta: RemoveBundleMeta,
    dst: u32,
}

/// Container for entities with any sets of components.
///
/// Entities can be spawned in the `World` with handle `Entity` returned,
/// that can be used later to access that entity.
///
/// `Entity` handle can be downgraded to `EntityId`.
///
/// Entity would be despawned after last `Entity` is dropped.
///
/// Entity's set of components may be modified in any way.
///
/// Entities can be fetched directly, using `Entity` or `EntityId`
/// with different guarantees and requirements.
///
/// Entities can be efficiently queried from `World` to iterate over all entities
/// that match query requirements.
///
/// Internally `World` manages entities generations,
/// maps entity to location of components in archetypes,
/// moves components of entities between archetypes,
/// spawns and despawns entities.
#[derive(Debug)]
pub struct World {
    /// Global epoch counter of the World.
    /// Incremented on each mutable query.
    epoch: u64,

    /// Collection of entities with their locations.
    entities: Entities,

    /// Archetypes of entities in the world.
    archetypes: Vec<Archetype>,

    /// Maps static key of the bundle to archetype index.
    keys: HashMap<TypeId, u32, NoOpHasherBuilder>,

    /// Maps ids list to archetype.
    ids: HashMap<Vec<TypeId>, u32, MulHasherBuilder>,

    /// Maps archetype index + additional component id to the archetype index.
    add_one: HashMap<(u32, TypeId), InsertInfo, MulHasherBuilder>,

    /// Maps archetype index + additional static bundle key to the archetype index.
    add_key: HashMap<(u32, TypeId), InsertBundleInfo, MulHasherBuilder>,

    /// Maps archetype index + additional component ids list to archetype.
    add_ids: HashMap<(u32, Vec<TypeId>), InsertBundleInfo, MulHasherBuilder>,

    /// Maps archetype index - removed component id to the archetype index.
    sub_one: HashMap<(u32, TypeId), RemoveInfo, MulHasherBuilder>,

    /// Maps archetype index + removed static bundle key to the archetype index.
    sub_key: HashMap<(u32, TypeId), RemoveBundleInfo, MulHasherBuilder>,

    /// Maps archetype index + removed component ids list to archetype.
    sub_ids: HashMap<(u32, Vec<TypeId>), RemoveBundleInfo, MulHasherBuilder>,

    /// Array of indices to drop.
    #[cfg(feature = "rc")]
    drop_queue: Vec<u32>,
}

impl Default for World {
    fn default() -> Self {
        World::new()
    }
}

impl World {
    /// Returns new instance of `World`.
    ///
    /// Created `World` instance contains no entities.
    ///
    /// Internal caches that make operations faster are empty.
    /// This can make a small spike in latency
    /// as each cache entry would be calculated on first use of each key.
    #[inline]
    pub fn new() -> Self {
        World {
            epoch: 0,
            #[cfg(feature = "rc")]
            entities: Entities::new(1024),
            #[cfg(not(feature = "rc"))]
            entities: Entities::new(),
            archetypes: Vec::new(),
            keys: HashMap::with_hasher(NoOpHasherBuilder),
            ids: HashMap::with_hasher(MulHasherBuilder),
            add_one: HashMap::with_hasher(MulHasherBuilder),
            add_key: HashMap::with_hasher(MulHasherBuilder),
            add_ids: HashMap::with_hasher(MulHasherBuilder),
            sub_one: HashMap::with_hasher(MulHasherBuilder),
            sub_key: HashMap::with_hasher(MulHasherBuilder),
            sub_ids: HashMap::with_hasher(MulHasherBuilder),
            #[cfg(feature = "rc")]
            drop_queue: Vec::new(),
        }
    }

    /// Spawns new entity in this world with provided bundle of components.
    /// World keeps ownership of the spawned entity and entity id is returned.
    #[inline]
    pub fn spawn<B>(&mut self, bundle: B) -> EntityId
    where
        B: DynamicBundle,
    {
        if !bundle.valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let entity = self.entities.spawn();

        let archetype_idx =
            cached_archetype_idx(&mut self.keys, &mut self.ids, &mut self.archetypes, &bundle);

        self.epoch += 1;
        let idx = self.archetypes[archetype_idx as usize].spawn(entity, bundle, self.epoch);
        self.entities.set_location(entity.idx, archetype_idx, idx);
        entity
    }

    /// Spawns new entity in this world with provided bundle of components.
    /// Returns owning reference to the entity.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn spawn_owning<B>(&mut self, bundle: B) -> Entity
    where
        B: DynamicBundle,
    {
        if !bundle.valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let entity = self.entities.spawn_owning();

        let archetype_idx =
            cached_archetype_idx(&mut self.keys, &mut self.ids, &mut self.archetypes, &bundle);

        self.epoch += 1;
        let idx = self.archetypes[archetype_idx as usize].spawn(*entity, bundle, self.epoch);
        self.entities.set_location(entity.idx, archetype_idx, idx);

        entity
    }

    /// Returns an iterator which spawns and yield entities
    /// using bundles returnd from provided iterator.
    ///
    /// When bundles iterator returns `None`, returned iterator returns `None` too.
    ///
    /// If bundles iterator is fused, returned iterator is fused too.
    /// If bundles iterator is double-ended, returned iterator is double-ended too.
    /// If bundles iterator has exact size, returned iterator has exact size too.
    ///
    /// Skipping items on returned iterator will not cause them to be spawned
    /// and same number of items will be skipped on bundles iterator.
    ///
    /// Returned iterator attempts to optimize storage allocation for entities
    /// if consumed with functions like `fold`, `rfold`, `for_each` or `collect`.
    ///
    /// When returned iterator is dropped, no more entities will be spawned
    /// even if bundles iterator has items left.
    #[inline]
    pub fn spawn_batch<B, I>(&mut self, bundles: I) -> SpawnBatch<'_, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let archetype_idx = cached_archetype_idx(
            &mut self.keys,
            &mut self.ids,
            &mut self.archetypes,
            &PhantomData::<I::Item>,
        );

        self.epoch += 1;

        let archetype = &mut self.archetypes[archetype_idx as usize];
        let entities = &mut self.entities;
        let epoch = self.epoch;

        SpawnBatch {
            bundles: bundles.into_iter(),
            epoch,
            archetype_idx,
            archetype,
            entities,
        }
    }

    /// Returns an iterator which spawns and yield entities
    /// using bundles returnd from provided iterator.
    ///
    /// When bundles iterator returns `None`, returned iterator returns `None` too.
    ///
    /// If bundles iterator is fused, returned iterator is fused too.
    /// If bundles iterator is double-ended, returned iterator is double-ended too.
    /// If bundles iterator has exact size, returned iterator has exact size too.
    ///
    /// Skipping items on returned iterator will not cause them to be spawned
    /// and same number of items will be skipped on bundles iterator.
    ///
    /// Returned iterator attempts to optimize storage allocation for entities
    /// if consumed with functions like `fold`, `rfold`, `for_each` or `collect`.
    ///
    /// When returned iterator is dropped, no more entities will be spawned
    /// even if bundles iterator has items left.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn spawn_batch_owning<B, I>(&mut self, bundles: I) -> SpawnBatchOwned<'_, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let archetype_idx = cached_archetype_idx(
            &mut self.keys,
            &mut self.ids,
            &mut self.archetypes,
            &PhantomData::<I::Item>,
        );

        self.epoch += 1;

        let archetype = &mut self.archetypes[archetype_idx as usize];
        let entities = &mut self.entities;
        let epoch = self.epoch;

        SpawnBatchOwned {
            bundles: bundles.into_iter(),
            epoch,
            archetype_idx,
            archetype,
            entities,
        }
    }

    /// Despawns an entity with specified id, currently owned by the `World`.
    #[cfg(feature = "rc")]
    pub fn despawn(&mut self, entity: &EntityId) -> Result<(), OwnershipError> {
        let (archetype, idx) = self.entities.despawn(entity)?;

        let opt_id = unsafe { self.archetypes[archetype as usize].despawn_unchecked(idx) };
        if let Some(id) = opt_id {
            self.entities.set_location(id, archetype, idx)
        }

        Ok(())
    }

    /// Despawns an entity with specified id.
    #[cfg(not(feature = "rc"))]
    pub fn despawn(&mut self, entity: &EntityId) -> Result<(), NoSuchEntity> {
        let (archetype, idx) = self.entities.despawn(entity)?;

        let opt_id = unsafe { self.archetypes[archetype as usize].despawn_unchecked(idx) };
        if let Some(id) = opt_id {
            self.entities.set_location(id, archetype, idx)
        }

        Ok(())
    }

    /// Transfers ownership of the entity from the caller to the `World`.
    /// After this call, entity won't be despawned until [`World::despawn`] is called with this entity id.
    #[cfg(feature = "rc")]
    pub fn keep<T>(&mut self, entity: Entity<T>) {
        assert!(self.entities.is_owner_of(&entity));
        self.entities.give_ownership(entity);
    }

    /// Transfers ownership of the entity from the `World` to the caller.
    /// After this call, entity should be despawned by dropping returned entity reference,
    /// or by returning ownership to the `World` and then called [`World::despawn`]
    ///
    /// Returns error if entity with specified id does not exists,
    /// or if that entity is not owned by the `World`.
    #[cfg(feature = "rc")]
    pub fn take(&mut self, entity: &EntityId) -> Result<Entity, OwnershipError> {
        self.entities.take_ownership(entity)
    }

    /// Inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn insert<T, P>(&mut self, entity: &Entity<P>, component: T)
    where
        T: Component,
    {
        assert!(self.entities.is_owner_of(entity));

        self.try_insert(entity, component).expect("Entity exists");
    }

    /// Attemots to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn try_insert<T>(&mut self, entity: &EntityId, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        let (archetype, idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;

        self.epoch += 1;

        if self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()) {
            unsafe {
                self.archetypes[archetype as usize].set(idx, component, self.epoch);
            }

            return Ok(());
        }

        let insert_info =
            cached_insert_info::<T>(&mut self.add_one, &mut self.archetypes, archetype);

        debug_assert_ne!(archetype, insert_info.dst);

        let (before, after) = self
            .archetypes
            .split_at_mut(archetype.max(insert_info.dst) as usize);

        let (src, dst) = match archetype < insert_info.dst {
            true => (&mut before[archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[insert_info.dst as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.insert(dst, idx, component, self.epoch) };

        self.entities
            .set_location(entity.idx, insert_info.dst, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, archetype, idx);
        }

        Ok(())
    }

    /// Removes component from the specified entity and returns its value.
    ///
    /// If entity does not have component of this type, fails with `Err(EntityError::MissingComponent)`.
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn remove<T>(&mut self, entity: &EntityId) -> Result<T, EntityError>
    where
        T: Component,
    {
        let (archetype, idx) = self.entities.get(entity).ok_or(EntityError::NoSuchEntity)?;

        self.epoch += 1;

        if !self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()) {
            return Err(EntityError::MissingComponents);
        }

        let remove_info =
            cached_remove_info::<T>(&mut self.sub_one, &mut self.archetypes, archetype);

        debug_assert_ne!(archetype, remove_info.dst);

        let (before, after) = self
            .archetypes
            .split_at_mut(archetype.max(remove_info.dst) as usize);

        let (src, dst) = match archetype < remove_info.dst {
            true => (&mut before[archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[remove_info.dst as usize]),
        };

        let (dst_idx, opt_src_id, component) = unsafe { src.remove(dst, idx) };

        self.entities
            .set_location(entity.idx, remove_info.dst, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, archetype, idx);
        }

        Ok(component)
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn insert_bundle<B, T>(&mut self, entity: &Entity<T>, bundle: B)
    where
        B: DynamicBundle,
    {
        assert!(self.entities.is_owner_of(entity));
        self.try_insert_bundle(entity, bundle).unwrap();
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn try_insert_bundle<B>(&mut self, entity: &EntityId, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        if !bundle.valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let (archetype, idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;

        if bundle.with_ids(|ids| ids.is_empty()) {
            return Ok(());
        }

        self.epoch += 1;

        let insert_info = cached_insert_bundle_info(
            &mut self.add_key,
            &mut self.add_ids,
            &mut self.archetypes,
            archetype,
            &bundle,
        );

        if insert_info.dst == archetype {
            unsafe { self.archetypes[archetype as usize].set_bundle(idx, bundle, self.epoch) };
            return Ok(());
        }

        let (before, after) = self
            .archetypes
            .split_at_mut(archetype.max(insert_info.dst) as usize);

        let (src, dst) = match archetype < insert_info.dst {
            true => (&mut before[archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.insert_bundle(dst, idx, bundle, self.epoch) };

        self.entities
            .set_location(entity.idx, insert_info.dst, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, archetype, idx);
        }

        Ok(())
    }

    /// Drops components of the specified entity with type from the bundle.
    /// Skips any component type entity doesn't have.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn remove_bundle<B>(&mut self, entity: &EntityId) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let (archetype, idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;

        if B::static_with_ids(|ids| {
            ids.iter()
                .all(|&id| !self.archetypes[archetype as usize].contains_id(id))
        }) {
            // No components to remove.
            return Ok(());
        }

        self.epoch += 1;

        let remove_info = cached_remove_bundle_info::<B>(
            &mut self.sub_key,
            &mut self.sub_ids,
            &mut self.archetypes,
            archetype,
        );

        debug_assert_ne!(archetype, remove_info.dst);

        let (before, after) = self
            .archetypes
            .split_at_mut(archetype.max(remove_info.dst) as usize);

        let (src, dst) = match archetype < remove_info.dst {
            true => (&mut before[archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.drop_bundle(dst, idx) };

        self.entities
            .set_location(entity.idx, remove_info.dst, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, archetype, idx);
        }

        Ok(())
    }

    /// Checks that entity has components of all types from the bundle.
    /// Pins those types to the entity.
    ///
    /// Pinning serves as API level contract.
    ///
    /// Pinned components are not enforced to stay at entity
    /// and can be removed using `World::remove` or `World::remove_bundle`
    /// with a clone of the `Entity` without component types pinned.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn pin_bundle<B>(&mut self, entity: Entity) -> Entity<B>
    where
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        assert!(self.entities.is_owner_of(&entity));

        let (archetype, _idx) = self.entities.get(&entity).unwrap();

        let archetype = &self.archetypes[archetype as usize];

        if B::static_with_ids(|ids| ids.iter().any(|&id| !archetype.contains_id(id))) {
            panic!("Attampt to pin missing components");
        }

        entity.with_bundle()
    }

    /// Queries components from specified entity.
    ///
    /// Requires access to all components in query.
    /// Components proved by entity reference must be queried or skipped in order
    /// followed by list of optionals.
    ///
    /// # Panics
    ///
    /// If `Entity` was not created by this world, this function will panic.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn get<'a, Q, A: 'a>(&'a self, entity: &Entity<A>) -> <Q::Fetch as Fetch<'a>>::Item
    where
        &'a A: Proof<Q>,
        Q: Query + ImmutableQuery + NonTrackingQuery,
    {
        assert!(self.entities.is_owner_of(entity));

        assert!(
            !Q::mutates(),
            "Invalid impl of `ImmutableQuery` for `{}`",
            type_name::<Q>()
        );

        assert!(
            !Q::tracks(),
            "Invalid impl of `NonTrackingQuery` for `{}`",
            type_name::<Q>()
        );

        let (archetype, idx) = self.entities.get(entity).unwrap();
        let archetype = &self.archetypes[archetype as usize];
        let mut fetch = unsafe { Q::fetch(archetype, 0, self.epoch) }.expect("Query is prooven");
        let item = unsafe { fetch.get_item(idx as usize) };
        item
    }

    /// Queries components from specified entity.
    ///
    /// Requires access to all components in query.
    /// Components proved by entity reference must be queried or skipped in order
    /// followed by list of optionals.
    ///
    /// # Panics
    ///
    /// If `Entity` was not created by this world, this function will panic.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn get_mut<'a, Q, A: 'a>(&'a mut self, entity: &Entity<A>) -> <Q::Fetch as Fetch<'a>>::Item
    where
        &'a mut A: Proof<Q>,
        Q: Query + NonTrackingQuery,
    {
        assert!(self.entities.is_owner_of(entity));

        assert!(
            !Q::tracks(),
            "Invalid impl of `NonTrackingQuery` for `{}`",
            type_name::<Q>()
        );

        if Q::mutates() {
            self.epoch += 1;
        }

        let (archetype, idx) = self.entities.get(entity).unwrap();
        let archetype = &self.archetypes[archetype as usize];
        let mut fetch = unsafe { Q::fetch(archetype, 0, self.epoch) }.expect("Query is prooven");
        let item = unsafe { fetch.get_item(idx as usize) };
        item
    }

    /// Queries components from specified entity.
    ///
    /// If query cannot be satisfied, returns `EntityError::MissingComponents`.
    #[inline]
    pub fn query_one<'a, Q>(
        &'a self,
        entity: &EntityId,
    ) -> Result<<Q::Fetch as Fetch<'a>>::Item, EntityError>
    where
        Q: Query + ImmutableQuery + NonTrackingQuery,
    {
        assert!(
            !Q::mutates(),
            "Invalid impl of `ImmutableQuery` for `{}`",
            type_name::<Q>()
        );

        assert!(
            !Q::tracks(),
            "Invalid impl of `NonTrackingQuery` for `{}`",
            type_name::<Q>()
        );

        let (archetype, idx) = self.entities.get(entity).ok_or(EntityError::NoSuchEntity)?;
        let archetype = &self.archetypes[archetype as usize];
        match unsafe { Q::fetch(archetype, 0, self.epoch) } {
            None => Err(EntityError::MissingComponents),
            Some(mut fetch) => {
                let item = unsafe { fetch.get_item(idx as usize) };
                Ok(item)
            }
        }
    }

    /// Queries components from specified entity.
    ///
    /// If query cannot be satisfied, returns `EntityError::MissingComponents`.
    #[inline]
    pub fn query_one_mut<'a, Q>(
        &'a mut self,
        entity: &EntityId,
    ) -> Result<<Q::Fetch as Fetch<'a>>::Item, EntityError>
    where
        Q: Query + NonTrackingQuery,
    {
        assert!(
            !Q::tracks(),
            "Invalid impl of `NonTrackingQuery` for `{}`",
            type_name::<Q>()
        );

        let (archetype, idx) = self.entities.get(entity).ok_or(EntityError::NoSuchEntity)?;
        let archetype = &self.archetypes[archetype as usize];
        match unsafe { Q::fetch(archetype, 0, self.epoch) } {
            None => Err(EntityError::MissingComponents),
            Some(mut fetch) => {
                let item = unsafe { fetch.get_item(idx as usize) };
                Ok(item)
            }
        }
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method only works with immutable queries that does not track for component changes.
    #[inline]
    pub fn query<'a, Q>(&'a self) -> QueryIter<'a, Q>
    where
        Q: Query + NonTrackingQuery + ImmutableQuery,
    {
        QueryIter::new(self.epoch, &self.archetypes)
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method can be used for queries that track for component changes.
    /// This method only works with immutable queries.
    #[inline]
    pub fn query_tracked<'a, Q>(&'a self, tracks: &mut Tracks) -> QueryTrackedIter<'a, Q>
    where
        Q: Query + ImmutableQuery,
    {
        let iter = QueryTrackedIter::new(tracks.epoch, self.epoch, &self.archetypes);
        tracks.epoch = self.epoch;
        iter
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method can be used for queries that mutate components.
    /// This method only works queries that does not track for component changes.
    #[inline]
    pub fn query_mut<'a, Q>(&'a mut self) -> QueryIter<'a, Q>
    where
        Q: Query + NonTrackingQuery,
    {
        if Q::mutates() {
            self.epoch += 1
        };

        QueryIter::new(self.epoch, &self.archetypes)
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method can be used for queries that mutates components and track for component changes.
    #[inline]
    pub fn query_tracked_mut<'a, Q>(&'a mut self, tracks: &mut Tracks) -> QueryTrackedIter<'a, Q>
    where
        Q: Query,
    {
        if Q::mutates() {
            self.epoch += 1
        };

        let iter = QueryTrackedIter::new(tracks.epoch, self.epoch, &self.archetypes);
        tracks.epoch = self.epoch;
        iter
    }

    /// Checks if specified entity has componet of specified type.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn has_component_owning<T: 'static, U>(&self, entity: &Entity<U>) -> bool {
        assert!(self.entities.is_owner_of(entity));

        let (archetype, _idx) = self.entities.get(entity).unwrap();
        self.archetypes[archetype as usize].contains_id(TypeId::of::<T>())
    }

    /// Attemtps to check if specified entity has componet of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn has_component<T: 'static>(&self, entity: &EntityId) -> Result<bool, NoSuchEntity> {
        let (archetype, _idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;
        Ok(self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()))
    }

    /// Checks if specified entity is still alive.
    #[inline]
    pub fn is_alive(&self, entity: &EntityId) -> bool {
        self.entities.get(entity).is_some()
    }

    /// Returns new [`Tracks`] instance to use with tracking queries.
    ///
    /// Returnd [`Tracks`] instance considers all modifications
    /// since creation of the world as "new" for the first tracking query.
    #[inline]
    pub fn tracks(&self) -> Tracks {
        Tracks { epoch: 0 }
    }

    /// Returns new [`Tracks`] instance to use with tracking queries.
    ///
    /// Returnd [`Tracks`] instance considers only modifications
    /// that happen after this function call as "new" for the first tracking query.
    #[inline]
    pub fn tracks_now(&self) -> Tracks {
        Tracks { epoch: self.epoch }
    }

    /// Run world maintenance, completing all deferred operations on it.
    ///
    /// Currently deferred operations are:
    /// * Despawn of entities with no strong references left
    #[inline]
    pub fn maintain(&mut self) {
        #[cfg(feature = "rc")]
        {
            let queue = self.entities.drop_queue();

            loop {
                queue.drain(&mut self.drop_queue);

                if self.drop_queue.is_empty() {
                    break;
                }

                for id in self.drop_queue.drain(..) {
                    let (archetype, idx) = self.entities.dropped(id);
                    let opt_id =
                        unsafe { self.archetypes[archetype as usize].despawn_unchecked(idx) };
                    if let Some(id) = opt_id {
                        self.entities.set_location(id, archetype, idx)
                    }
                }
            }
        }
    }

    /// Iterates through world using specified query.
    ///
    /// This method only works with immutable queries that does not track for component changes.
    #[inline]
    pub fn for_each<Q, F>(&self, mut f: F)
    where
        Q: Query + NonTrackingQuery + ImmutableQuery,
        F: FnMut(QueryItem<'_, Q>),
    {
        debug_assert!(!Q::mutates());

        for archetype in &self.archetypes {
            if let Some(mut fetch) = unsafe { Q::fetch(archetype, 0, self.epoch) } {
                for idx in 0..archetype.len() {
                    f(unsafe { fetch.get_item(idx) });
                }
            }
        }
    }

    /// Iterates through world using specified query.
    ///
    /// This method can be used for queries that track for component changes.
    /// This method only works with immutable queries.
    #[inline]
    pub fn for_each_tracked<Q, F>(&self, tracks: &mut Tracks, mut f: F)
    where
        Q: Query + ImmutableQuery,
        F: FnMut(QueryItem<'_, Q>),
    {
        debug_assert!(!Q::mutates());

        let tracks_epoch = tracks.epoch;
        tracks.epoch = self.epoch;

        for archetype in &self.archetypes {
            if let Some(mut fetch) = unsafe { Q::fetch(archetype, tracks_epoch, self.epoch) } {
                for chunk_idx in 0..archetype.len() / CHUNK_LEN_USIZE {
                    if unsafe { fetch.skip_chunk(chunk_idx) } {
                        continue;
                    }

                    for idx in
                        chunk_idx * CHUNK_LEN_USIZE..chunk_idx * CHUNK_LEN_USIZE + CHUNK_LEN_USIZE
                    {
                        if !unsafe { fetch.skip_item(idx) } {
                            f(unsafe { fetch.get_item(idx) });
                        }
                    }
                }

                let tail = archetype.len() % CHUNK_LEN_USIZE;

                if tail > 0 {
                    let chunk_idx = archetype.len() / CHUNK_LEN_USIZE;
                    if unsafe { fetch.skip_chunk(chunk_idx) } {
                        continue;
                    }

                    for idx in chunk_idx * CHUNK_LEN_USIZE..chunk_idx * CHUNK_LEN_USIZE + tail {
                        if !unsafe { fetch.skip_item(idx) } {
                            f(unsafe { fetch.get_item(idx) });
                        }
                    }
                }
            }
        }
    }

    /// Iterates through world using specified query.
    ///
    /// This method can be used for queries that mutate components.
    /// This method only works queries that does not track for component changes.
    #[inline]
    pub fn for_each_mut<Q, F>(&mut self, mut f: F)
    where
        Q: Query + NonTrackingQuery,
        F: FnMut(QueryItem<'_, Q>),
    {
        if Q::mutates() {
            self.epoch += 1
        };

        for archetype in &self.archetypes {
            if let Some(mut fetch) = unsafe { Q::fetch(archetype, 0, self.epoch) } {
                if Q::mutates() {
                    for chunk_idx in 0..archetype.len() / CHUNK_LEN_USIZE {
                        debug_assert!(!unsafe { fetch.skip_chunk(chunk_idx) });
                        unsafe { fetch.visit_chunk(chunk_idx) }

                        let idx_begin = chunk_idx * CHUNK_LEN_USIZE;
                        for idx in idx_begin..idx_begin + CHUNK_LEN_USIZE {
                            debug_assert!(!unsafe { fetch.skip_item(idx) });
                            f(unsafe { fetch.get_item(idx) });
                        }
                    }

                    let tail = archetype.len() % CHUNK_LEN_USIZE;

                    if tail > 0 {
                        let chunk_idx = archetype.len() / CHUNK_LEN_USIZE;
                        debug_assert!(!unsafe { fetch.skip_chunk(chunk_idx) });
                        unsafe { fetch.visit_chunk(chunk_idx) }

                        let idx_begin = chunk_idx * CHUNK_LEN_USIZE;
                        for idx in idx_begin..idx_begin + tail {
                            debug_assert!(!unsafe { fetch.skip_item(idx) });
                            f(unsafe { fetch.get_item(idx) });
                        }
                    }
                } else {
                    for idx in 0..archetype.len() {
                        debug_assert!(!unsafe { fetch.skip_item(idx) });
                        f(unsafe { fetch.get_item(idx) });
                    }
                }
            }
        }
    }

    /// Iterates through world using specified query.
    ///
    /// This method can be used for queries that mutates components and track for component changes.
    #[inline]
    pub fn for_each_tracked_mut<Q, F>(&mut self, tracks: &mut Tracks, mut f: F)
    where
        Q: Query,
        F: FnMut(QueryItem<'_, Q>),
    {
        if Q::mutates() {
            self.epoch += 1
        };

        let tracks_epoch = tracks.epoch;
        tracks.epoch = self.epoch;

        for archetype in &self.archetypes {
            if let Some(mut fetch) = unsafe { Q::fetch(archetype, tracks_epoch, self.epoch) } {
                for chunk_idx in 0..archetype.len() / CHUNK_LEN_USIZE {
                    if unsafe { fetch.skip_chunk(chunk_idx) } {
                        continue;
                    }

                    if Q::mutates() {
                        unsafe { fetch.visit_chunk(chunk_idx) }
                    }

                    let idx_begin = chunk_idx * CHUNK_LEN_USIZE;
                    for idx in idx_begin..idx_begin + CHUNK_LEN_USIZE {
                        if !unsafe { fetch.skip_item(idx) } {
                            f(unsafe { fetch.get_item(idx) });
                        }
                    }
                }

                let tail = archetype.len() % CHUNK_LEN_USIZE;

                if tail > 0 {
                    let chunk_idx = archetype.len() / CHUNK_LEN_USIZE;
                    if unsafe { fetch.skip_chunk(chunk_idx) } {
                        continue;
                    }

                    if Q::mutates() {
                        unsafe { fetch.visit_chunk(chunk_idx) }
                    }

                    let idx_begin = chunk_idx * CHUNK_LEN_USIZE;
                    for idx in idx_begin..idx_begin + tail {
                        if !unsafe { fetch.skip_item(idx) } {
                            f(unsafe { fetch.get_item(idx) });
                        }
                    }
                }
            }
        }
    }
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
#[derive(Debug)]
pub struct SpawnBatch<'a, I> {
    bundles: I,
    epoch: u64,
    archetype_idx: u32,
    archetype: &'a mut Archetype,
    entities: &'a mut Entities,
}

impl<B, I> SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    /// Spawns the rest of the entities, dropping their ids.
    ///
    /// Note that `SpawnBatchOwned` does not have this mmethods
    /// as dropped `Entity` references would cause spawned entities
    /// to be despawned, and that's probably not what user wants.
    pub fn spawn_all(mut self) {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.for_each(|bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.idx, archetype_idx, idx);
        })
    }
}

impl<B, I> Iterator for SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = EntityId;

    fn next(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next()?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx, self.archetype_idx, idx);

        Some(entity)
    }

    fn nth(&mut self, n: usize) -> Option<EntityId> {
        // No reason to create entities
        // for which the only reference is immediatelly dropped
        let bundle = self.bundles.nth(n)?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx, self.archetype_idx, idx);

        Some(entity)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, EntityId) -> T,
    {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.fold(init, |acc, bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.idx, archetype_idx, idx);
            f(acc, entity)
        })
    }

    fn collect<T>(self) -> T
    where
        T: FromIterator<EntityId>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.
        spawn_reserve(&self.bundles, self.archetype);

        FromIterator::from_iter(self)
    }
}

impl<B, I> ExactSizeIterator for SpawnBatch<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle,
{
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<B, I> DoubleEndedIterator for SpawnBatch<'_, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next_back()?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx, self.archetype_idx, idx);

        Some(entity)
    }

    fn nth_back(&mut self, n: usize) -> Option<EntityId> {
        // No reason to create entities
        // for which the only reference is immediatelly dropped
        let bundle = self.bundles.nth_back(n)?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx, self.archetype_idx, idx);

        Some(entity)
    }

    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, EntityId) -> T,
    {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.rfold(init, |acc, bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.idx, archetype_idx, idx);
            f(acc, entity)
        })
    }
}

impl<B, I> FusedIterator for SpawnBatch<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle,
{
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
#[cfg(feature = "rc")]
#[derive(Debug)]
pub struct SpawnBatchOwned<'a, I> {
    bundles: I,
    epoch: u64,
    archetype_idx: u32,
    archetype: &'a mut Archetype,
    entities: &'a mut Entities,
}

#[cfg(feature = "rc")]
impl<B, I> Iterator for SpawnBatchOwned<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        let bundle = self.bundles.next()?;

        let entity = self.entities.spawn_owning();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx, self.archetype_idx, idx);

        Some(entity)
    }

    fn nth(&mut self, n: usize) -> Option<Entity> {
        // No reason to create entities
        // for which the only reference is immediatelly dropped
        let bundle = self.bundles.nth(n)?;

        let entity = self.entities.spawn_owning();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx, self.archetype_idx, idx);

        Some(entity)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, Entity) -> T,
    {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.fold(init, |acc, bundle| {
            let entity = entities.spawn_owning();
            let idx = archetype.spawn(*entity, bundle, epoch);
            entities.set_location(entity.idx, archetype_idx, idx);
            f(acc, entity)
        })
    }

    fn collect<T>(self) -> T
    where
        T: FromIterator<Entity>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.
        spawn_reserve(&self.bundles, self.archetype);

        FromIterator::from_iter(self)
    }
}

#[cfg(feature = "rc")]
impl<B, I> ExactSizeIterator for SpawnBatchOwned<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle,
{
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

#[cfg(feature = "rc")]
impl<B, I> DoubleEndedIterator for SpawnBatchOwned<'_, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<Entity> {
        let bundle = self.bundles.next_back()?;

        let entity = self.entities.spawn_owning();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx, self.archetype_idx, idx);

        Some(entity)
    }

    fn nth_back(&mut self, n: usize) -> Option<Entity> {
        // No reason to create entities
        // for which the only reference is immediatelly dropped
        let bundle = self.bundles.nth_back(n)?;

        let entity = self.entities.spawn_owning();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx, self.archetype_idx, idx);

        Some(entity)
    }

    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, Entity) -> T,
    {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.rfold(init, |acc, bundle| {
            let entity = entities.spawn_owning();
            let idx = archetype.spawn(*entity, bundle, epoch);
            entities.set_location(entity.idx, archetype_idx, idx);
            f(acc, entity)
        })
    }
}

#[cfg(feature = "rc")]
impl<B, I> FusedIterator for SpawnBatchOwned<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle,
{
}

/// Error returned in case specified [`EntityId`]
/// does not reference any live entity in the [`World`].
#[derive(Clone, Copy, Debug)]
pub struct NoSuchEntity;

impl fmt::Display for NoSuchEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Speicified entity is not found")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NoSuchEntity {}

/// Error returned in case specified entity does not contain
/// component of required type.
#[derive(Clone, Copy, Debug)]
pub struct MissingComponents;

impl fmt::Display for MissingComponents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Speicified component is not found in entity")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MissingComponents {}

/// Error returned if either entity reference is invalid
/// or component of required type is not found for an entity.
#[derive(Clone, Copy, Debug)]
pub enum EntityError {
    /// Error returned in case specified [`EntityId`]
    /// does not reference any live entity in the [`World`].
    NoSuchEntity,

    /// Error returned in case specified entity does not contain
    /// component of required type.
    MissingComponents,
}

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchEntity => fmt::Display::fmt(&NoSuchEntity, f),
            Self::MissingComponents => fmt::Display::fmt(&MissingComponents, f),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EntityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoSuchEntity => Some(&NoSuchEntity),
            Self::MissingComponents => Some(&MissingComponents),
        }
    }
}

impl From<NoSuchEntity> for EntityError {
    fn from(_: NoSuchEntity) -> Self {
        EntityError::NoSuchEntity
    }
}

impl From<MissingComponents> for EntityError {
    fn from(_: MissingComponents) -> Self {
        EntityError::MissingComponents
    }
}

/// Error that may occur when function expects `World` to own an entity with specific id.
#[cfg(feature = "rc")]
#[derive(Clone, Copy, Debug)]
pub enum OwnershipError {
    /// Error returned in case specified [`EntityId`]
    /// does not reference any live entity in the [`World`].
    NoSuchEntity,

    /// Error returned in case specified [`EntityId`]
    /// does not reference an entity currently owned by [`World`].
    NotOwned,
}

#[cfg(feature = "rc")]
impl From<NoSuchEntity> for OwnershipError {
    fn from(_: NoSuchEntity) -> Self {
        OwnershipError::NoSuchEntity
    }
}

#[cfg(feature = "rc")]
impl fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchEntity => fmt::Display::fmt(&NoSuchEntity, f),
            Self::NotOwned => f.write_str("Entity is not owned by World"),
        }
    }
}

#[cfg(feature = "rc")]
#[cfg(feature = "std")]
impl std::error::Error for OwnershipError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoSuchEntity => Some(&NoSuchEntity),
            _ => None,
        }
    }
}

/// Umbrella trait for [`DynamicBundle`] and [`Bundle`].
trait AsBundle {
    /// Returns static key if the bundle type have one.
    fn key() -> Option<TypeId>;

    /// Calls provided closure with slice of ids of types that this bundle contains.
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R;

    /// Calls provided closure with slice of component infos of types that this bundle contains.
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R;
}

impl<B> AsBundle for B
where
    B: DynamicBundle,
{
    fn key() -> Option<TypeId> {
        <B as DynamicBundle>::key()
    }

    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
        DynamicBundle::with_ids(self, f)
    }

    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
        DynamicBundle::with_components(self, f)
    }
}

impl<B> AsBundle for PhantomData<B>
where
    B: Bundle,
{
    fn key() -> Option<TypeId> {
        Some(B::static_key())
    }

    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
        B::static_with_ids(f)
    }

    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
        B::static_with_components(f)
    }
}

fn make_archetype_idx<B>(archetypes: &mut Vec<Archetype>, bundle: &B) -> u32
where
    B: AsBundle,
{
    match archetypes
        .iter()
        .position(|a| bundle.with_ids(|ids| a.matches(ids.iter().copied())))
    {
        None => {
            assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

            let archetype = bundle.with_components(|infos| Archetype::new(infos.iter()));
            archetypes.push(archetype);
            let idx = archetypes.len() - 1;
            idx as u32
        }
        Some(idx) => idx as u32,
    }
}

fn get_archetype_idx<B>(
    map: &mut HashMap<Vec<TypeId>, u32, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    bundle: &B,
) -> u32
where
    B: AsBundle,
{
    let raw_entry = bundle.with_ids(|ids| map.raw_entry_mut().from_key(ids));

    match raw_entry {
        RawEntryMut::Occupied(entry) => *entry.get(),
        RawEntryMut::Vacant(entry) => {
            let idx = make_archetype_idx(archetypes, bundle);
            entry.insert(bundle.with_ids(|ids| ids.into()), idx);
            idx
        }
    }
}

fn cached_archetype_idx<B>(
    keys: &mut HashMap<TypeId, u32, NoOpHasherBuilder>,
    ids: &mut HashMap<Vec<TypeId>, u32, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    bundle: &B,
) -> u32
where
    B: AsBundle,
{
    match B::key() {
        None => get_archetype_idx(ids, archetypes, bundle),
        Some(key) => match keys.entry(key) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = get_archetype_idx(ids, archetypes, bundle);
                entry.insert(idx);
                idx
            }
        },
    }
}

fn make_insert_info<T>(archetypes: &mut Vec<Archetype>, src: u32) -> InsertInfo
where
    T: Component,
{
    match archetypes.iter().position(|a| {
        let ids = archetypes[src as usize]
            .ids()
            .chain(Some(TypeId::of::<T>()));
        a.matches(ids)
    }) {
        None => {
            assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

            let archetype = Archetype::new(
                archetypes[src as usize]
                    .infos()
                    .chain(Some(&ComponentInfo::of::<T>())),
            );

            // let meta = InsertMeta::new::<T>(&archetypes[src as usize], &archetype);

            archetypes.push(archetype);

            InsertInfo {
                // meta,
                dst: archetypes.len() as u32 - 1,
            }
        }
        Some(idx) => {
            // let meta = InsertMeta::new::<T>(&archetypes[src as usize], &archetypes[idx]);

            InsertInfo {
                // meta,
                dst: idx as u32,
            }
        }
    }
}

fn cached_insert_info<'a, T>(
    keys: &'a mut HashMap<(u32, TypeId), InsertInfo, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
) -> &'a InsertInfo
where
    T: Component,
{
    match keys.entry((src, TypeId::of::<T>())) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let info = make_insert_info::<T>(archetypes, src);
            entry.insert(info.clone())
        }
    }
}

fn make_insert_bundle_info<B>(
    archetypes: &mut Vec<Archetype>,
    src: u32,
    bundle: &B,
) -> InsertBundleInfo
where
    B: DynamicBundle,
{
    match archetypes.iter().position(|a| {
        bundle.with_ids(|ids| {
            let ids = archetypes[src as usize].ids().chain(ids.iter().copied());
            a.matches(ids)
        })
    }) {
        None => {
            assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

            let archetype = bundle.with_components(|infos| {
                Archetype::new(
                    archetypes[src as usize]
                        .infos()
                        .filter(|info| !bundle.contains_id(info.id))
                        .chain(infos),
                )
            });

            // let meta = InsertBundleMeta::new(&archetypes[src as usize], &archetype, bundle);

            archetypes.push(archetype);

            InsertBundleInfo {
                // meta,
                dst: archetypes.len() as u32 - 1,
            }
        }
        Some(idx) => {
            // let meta = InsertBundleMeta::new(&archetypes[src as usize], &archetypes[idx], bundle);

            InsertBundleInfo {
                // meta,
                dst: idx as u32,
            }
        }
    }
}

fn get_insert_bundle_info<'a, B>(
    map: &'a mut HashMap<(u32, Vec<TypeId>), InsertBundleInfo, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
    bundle: &B,
) -> &'a InsertBundleInfo
where
    B: DynamicBundle,
{
    let raw_entry = bundle.with_ids(move |ids| {
        let mut hasher = map.hasher().build_hasher();
        (src, ids).hash(&mut hasher);
        let hash = hasher.finish();

        map.raw_entry_mut()
            .from_hash(hash, |(key_src, key_ids)| *key_src == src && key_ids == ids)
    });

    match raw_entry {
        RawEntryMut::Occupied(entry) => entry.into_mut(),
        RawEntryMut::Vacant(entry) => {
            let info = make_insert_bundle_info(archetypes, src, bundle);
            let (_, info) = entry.insert(bundle.with_ids(|ids| (src, ids.into())), info);
            info
        }
    }
}

fn cached_insert_bundle_info<'a, B>(
    keys: &'a mut HashMap<(u32, TypeId), InsertBundleInfo, MulHasherBuilder>,
    ids: &'a mut HashMap<(u32, Vec<TypeId>), InsertBundleInfo, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
    bundle: &B,
) -> &'a InsertBundleInfo
where
    B: DynamicBundle,
{
    match B::key() {
        None => get_insert_bundle_info(ids, archetypes, src, bundle),
        Some(key) => match keys.entry((src, key)) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let info = get_insert_bundle_info(ids, archetypes, src, bundle);
                entry.insert(info.clone())
            }
        },
    }
}

fn make_remove_info<T>(archetypes: &mut Vec<Archetype>, src: u32) -> RemoveInfo
where
    T: Component,
{
    match archetypes.iter().position(|a| {
        let ids = archetypes[src as usize]
            .ids()
            .filter(|id| *id != TypeId::of::<T>());
        a.matches(ids)
    }) {
        None => {
            assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

            let archetype = Archetype::new(
                archetypes[src as usize]
                    .infos()
                    .filter(|info| info.id != TypeId::of::<T>()),
            );

            // let meta = RemoveMeta::new::<T>(&archetypes[src as usize], &archetype);

            archetypes.push(archetype);

            RemoveInfo {
                // meta,
                dst: archetypes.len() as u32 - 1,
            }
        }
        Some(idx) => {
            // let meta = RemoveMeta::new::<T>(&archetypes[src as usize], &archetypes[idx]);

            RemoveInfo {
                // meta,
                dst: idx as u32,
            }
        }
    }
}

fn cached_remove_info<'a, T>(
    keys: &'a mut HashMap<(u32, TypeId), RemoveInfo, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
) -> &'a RemoveInfo
where
    T: Component,
{
    match keys.entry((src, TypeId::of::<T>())) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let info = make_remove_info::<T>(archetypes, src);
            entry.insert(info.clone())
        }
    }
}

fn make_remove_bundle_info<B>(archetypes: &mut Vec<Archetype>, src: u32) -> RemoveBundleInfo
where
    B: Bundle,
{
    let ids = archetypes[src as usize]
        .ids()
        .filter(|id| !B::static_contains_id(*id));

    match archetypes.iter().position(|a| a.matches(ids.clone())) {
        None => {
            drop(ids);

            assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

            let archetype = Archetype::new(
                archetypes[src as usize]
                    .infos()
                    .filter(|info| !B::static_contains_id(info.id)),
            );

            // let meta = RemoveBundleMeta::new::<B>(&archetypes[src as usize], &archetype);
            archetypes.push(archetype);

            RemoveBundleInfo {
                // meta,
                dst: archetypes.len() as u32 - 1,
            }
        }
        Some(idx) => {
            // let meta = RemoveBundleMeta::new::<B>(&archetypes[src as usize], &archetypes[idx]);

            RemoveBundleInfo {
                // meta,
                dst: idx as u32,
            }
        }
    }
}

fn get_remove_bundle_info<'a, B>(
    map: &'a mut HashMap<(u32, Vec<TypeId>), RemoveBundleInfo, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
) -> &'a RemoveBundleInfo
where
    B: Bundle,
{
    let raw_entry = B::static_with_ids(move |ids| {
        let mut hasher = map.hasher().build_hasher();
        (src, ids).hash(&mut hasher);
        let hash = hasher.finish();

        map.raw_entry_mut()
            .from_hash(hash, |(key_src, key_ids)| *key_src == src && key_ids == ids)
    });

    match raw_entry {
        RawEntryMut::Occupied(entry) => entry.into_mut(),
        RawEntryMut::Vacant(entry) => {
            let info = make_remove_bundle_info::<B>(archetypes, src);
            let (_, info) = entry.insert(B::static_with_ids(|ids| (src, ids.into())), info);
            info
        }
    }
}

fn cached_remove_bundle_info<'a, B>(
    keys: &'a mut HashMap<(u32, TypeId), RemoveBundleInfo, MulHasherBuilder>,
    ids: &'a mut HashMap<(u32, Vec<TypeId>), RemoveBundleInfo, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
) -> &'a RemoveBundleInfo
where
    B: Bundle,
{
    match B::key() {
        None => get_remove_bundle_info::<B>(ids, archetypes, src),
        Some(key) => match keys.entry((src, key)) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let info = get_remove_bundle_info::<B>(ids, archetypes, src);
                entry.insert(info.clone())
            }
        },
    }
}
