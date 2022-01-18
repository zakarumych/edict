use core::{
    any::{type_name, TypeId},
    hash::{BuildHasher, Hash, Hasher},
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
    entity::{Entities, Entity, WeakEntity},
    hash::{MulHasherBuilder, NoOpHasherBuilder},
    idx::MAX_IDX_USIZE,
    proof::Proof,
    query::{
        Fetch, ImmutableQuery, NonTrackingQuery, Query, QueryItem, QueryIter, QueryTrackedIter,
    },
    tracks::Tracks,
};

/// Container for entities with any sets of components.
///
/// Entities can be spawned in the `World` with handle `Entity` returned,
/// that can be used later to access that entity.
///
/// `Entity` handle can be downgraded to `WeakEntity`.
///
/// Entity would be despawned after last `Entity` is dropped.
///
/// Entity's set of components may be modified in any way.
///
/// Entities can be fetched directly, using `Entity` or `WeakEntity`
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

    /// Maps archetype index + additional static bundle key to the archetype index.
    add_key: HashMap<(u32, TypeId), u32, MulHasherBuilder>,

    /// Maps archetype index + ids list to archetype.
    add_ids: HashMap<(u32, Vec<TypeId>), u32, MulHasherBuilder>,

    /// Maps archetype index - additional component id to the archetype index.
    sub_key: HashMap<(u32, TypeId), u32, MulHasherBuilder>,

    /// Maps archetype index - additional component id to the archetype index.
    sub_ids: HashMap<(u32, Vec<TypeId>), u32, MulHasherBuilder>,
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
            entities: Entities::new(1024),
            archetypes: Vec::new(),
            keys: HashMap::with_hasher(NoOpHasherBuilder),
            ids: HashMap::with_hasher(MulHasherBuilder),
            add_key: HashMap::with_hasher(MulHasherBuilder),
            add_ids: HashMap::with_hasher(MulHasherBuilder),
            sub_key: HashMap::with_hasher(MulHasherBuilder),
            sub_ids: HashMap::with_hasher(MulHasherBuilder),
        }
    }

    #[inline]
    pub fn spawn<B>(&mut self, bundle: B) -> Entity
    where
        B: DynamicBundle,
    {
        let entity = self.entities.spawn();

        let archetype_idx = get_archetype_idx_with_maps(
            &mut self.keys,
            &mut self.ids,
            &mut self.archetypes,
            &bundle,
        );

        self.epoch += 1;
        let idx = self.archetypes[archetype_idx as usize].spawn(*entity, bundle, self.epoch, 0);
        self.entities.set_location(entity.id, archetype_idx, idx);
        entity
    }

    #[inline]
    pub fn spawn_batch<I>(&mut self, bundles: I) -> SpawnBundle<'_, I::Item, I::IntoIter>
    where
        I: IntoIterator,
        I::Item: Bundle,
    {
        let archetype_idx = get_archetype_idx_with_maps(
            &mut self.keys,
            &mut self.ids,
            &mut self.archetypes,
            &PhantomData::<I::Item>,
        );

        self.epoch += 1;

        let archetype = &mut self.archetypes[archetype_idx as usize];
        let entities = &mut self.entities;
        let epoch = self.epoch;

        SpawnBundle {
            bundles: bundles.into_iter(),
            epoch,
            archetype_idx,
            archetype,
            entities,
        }
    }

    #[inline]
    pub fn insert<T, P>(&mut self, entity: &Entity<P>, component: T)
    where
        T: Component,
    {
        assert!(self.entities.is_owner_of(entity));

        self.try_insert(entity, component).expect("Entity exists");
    }

    #[inline]
    pub fn try_insert<T>(&mut self, entity: &WeakEntity, component: T) -> Result<(), NoSuchEntity>
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

        let dst_archetype = get_add_archetype_idx_with_maps(
            &mut self.add_key,
            &mut self.add_ids,
            &mut self.archetypes,
            archetype,
            &PhantomData::<(T,)>,
        );

        debug_assert_ne!(archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(archetype.max(dst_archetype) as usize);

        let (src, dst) = match archetype < dst_archetype {
            true => (&mut before[archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.insert(dst, idx, component, self.epoch) };

        self.entities
            .set_location(entity.id, dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, archetype, idx);
        }

        Ok(())
    }

    #[inline]
    pub fn remove<T>(&mut self, entity: &WeakEntity) -> Result<T, EntityError>
    where
        T: Component,
    {
        let (archetype, idx) = self.entities.get(entity).ok_or(EntityError::NoSuchEntity)?;

        self.epoch += 1;

        if !self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()) {
            return Err(EntityError::MissingComponents);
        }

        let dst_archetype = get_sub_archetype_idx_with_maps(
            &mut self.sub_key,
            &mut self.sub_ids,
            &mut self.archetypes,
            archetype,
            &PhantomData::<(T,)>,
        );

        debug_assert_ne!(archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(archetype.max(dst_archetype) as usize);

        let (src, dst) = match archetype < dst_archetype {
            true => (&mut before[archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id, component) = unsafe { src.remove(dst, idx) };

        self.entities
            .set_location(entity.id, dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, archetype, idx);
        }

        Ok(component)
    }

    #[inline]
    pub fn insert_bundle<B, T>(&mut self, entity: &Entity<T>, bundle: B)
    where
        B: DynamicBundle,
    {
        assert!(self.entities.is_owner_of(entity));
        self.try_insert_bundle(entity, bundle).unwrap();
    }

    #[inline]
    pub fn try_insert_bundle<B>(
        &mut self,
        entity: &WeakEntity,
        bundle: B,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        let (archetype, idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;

        if bundle.with_ids(|ids| ids.is_empty()) {
            return Ok(());
        }

        self.epoch += 1;

        let dst_archetype = get_add_archetype_idx_with_maps(
            &mut self.add_key,
            &mut self.add_ids,
            &mut self.archetypes,
            archetype,
            &bundle,
        );

        if dst_archetype == archetype {
            unsafe { self.archetypes[archetype as usize].set_bundle(idx, bundle, self.epoch) };
            return Ok(());
        }

        let (before, after) = self
            .archetypes
            .split_at_mut(archetype.max(dst_archetype) as usize);

        let (src, dst) = match archetype < dst_archetype {
            true => (&mut before[archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.insert_bundle(dst, idx, bundle, self.epoch) };

        self.entities
            .set_location(entity.id, dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, archetype, idx);
        }

        Ok(())
    }

    #[inline]
    pub fn remove_bundle<B>(&mut self, entity: &WeakEntity) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        let (archetype, idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;

        if B::static_with_ids(|ids| {
            ids.iter()
                .all(|&id| !self.archetypes[archetype as usize].contains_id(id))
        }) {
            // No components to remove.
            return Ok(());
        }

        self.epoch += 1;

        let dst_archetype = get_sub_archetype_idx_with_maps(
            &mut self.sub_key,
            &mut self.sub_ids,
            &mut self.archetypes,
            archetype,
            &PhantomData::<B>,
        );

        debug_assert_ne!(archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(archetype.max(dst_archetype) as usize);

        let (src, dst) = match archetype < dst_archetype {
            true => (&mut before[archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.drop_bundle(dst, idx) };

        self.entities
            .set_location(entity.id, dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, archetype, idx);
        }

        Ok(())
    }

    #[inline]
    pub fn pin_bundle<B>(&mut self, entity: Entity) -> Entity<B>
    where
        B: Bundle,
    {
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
        entity: &WeakEntity,
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
        entity: &WeakEntity,
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

    #[inline]
    pub fn query<'a, Q>(&'a self) -> QueryIter<'a, Q>
    where
        Q: Query + NonTrackingQuery + ImmutableQuery,
    {
        QueryIter::new(self.epoch, &self.archetypes)
    }

    #[inline]
    pub fn query_tracked<'a, Q>(&'a self, tracks: &mut Tracks) -> QueryTrackedIter<'a, Q>
    where
        Q: Query + ImmutableQuery,
    {
        let iter = QueryTrackedIter::new(tracks.epoch, self.epoch, &self.archetypes);
        tracks.epoch = self.epoch;
        iter
    }

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

    #[inline]
    pub fn has_component<T: 'static, U>(&self, entity: &Entity<U>) -> bool {
        assert!(self.entities.is_owner_of(entity));

        let (archetype, _idx) = self.entities.get(entity).unwrap();
        self.archetypes[archetype as usize].contains_id(TypeId::of::<T>())
    }

    #[inline]
    pub fn has_component_weak<T: 'static>(
        &self,
        entity: &WeakEntity,
    ) -> Result<bool, NoSuchEntity> {
        let (archetype, _idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;
        Ok(self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()))
    }

    #[inline]
    pub fn is_alive(&self, entity: &WeakEntity) -> bool {
        self.entities.get(entity).is_some()
    }

    #[inline]
    pub fn tracks(&self) -> Tracks {
        Tracks { epoch: 0 }
    }

    #[inline]
    pub fn tracks_now(&self) -> Tracks {
        Tracks { epoch: self.epoch }
    }

    #[inline]
    pub fn maintain(&mut self) {
        let queue = self.entities.drop_queue();

        for id in queue.drain() {
            let (archetype, idx) = self.entities.dropped(id);
            let opt_id = unsafe { self.archetypes[archetype as usize].despawn_unchecked(idx) };
            if let Some(id) = opt_id {
                self.entities.set_location(id, archetype, idx)
            }
        }
    }

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
}

pub struct SpawnBundle<'a, B: Bundle, I: Iterator<Item = B>> {
    bundles: I,
    epoch: u64,
    archetype_idx: u32,
    archetype: &'a mut Archetype,
    entities: &'a mut Entities,
}

impl<B, I> Iterator for SpawnBundle<'_, B, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        let bundle = self.bundles.next()?;

        let (lower, upper) = self.bundles.size_hint();

        let reserve = match upper {
            None => lower,
            Some(upper) => upper.min(lower * 2),
        };

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch, reserve);

        self.entities
            .set_location(entity.id, self.archetype_idx, idx);

        Some(entity)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }
}

impl<B, I> ExactSizeIterator for SpawnBundle<'_, B, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle,
{
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<B, I> DoubleEndedIterator for SpawnBundle<'_, B, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<Entity> {
        let bundle = self.bundles.next_back()?;

        let (lower, upper) = self.bundles.size_hint();

        let reserve = match upper {
            None => lower,
            Some(upper) => upper.min(lower * 2),
        };

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch, reserve);

        self.entities
            .set_location(entity.id, self.archetype_idx, idx);

        Some(entity)
    }
}

impl<B, I> FusedIterator for SpawnBundle<'_, B, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle,
{
}

#[derive(Debug)]
pub struct NoSuchEntity;

#[derive(Debug)]
pub struct MissingComponents;

#[derive(Debug)]
pub enum EntityError {
    NoSuchEntity,
    MissingComponents,
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

pub trait AsBundle {
    fn key() -> Option<TypeId>;
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R;
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

fn get_archetype_idx<B>(archetypes: &mut Vec<Archetype>, bundle: &B) -> u32
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

fn get_archetype_idx_with_idx_map<B>(
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
            let idx = get_archetype_idx(archetypes, bundle);
            entry.insert(bundle.with_ids(|ids| ids.into()), idx);
            idx
        }
    }
}

fn get_archetype_idx_with_maps<B>(
    keys: &mut HashMap<TypeId, u32, NoOpHasherBuilder>,
    ids: &mut HashMap<Vec<TypeId>, u32, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    bundle: &B,
) -> u32
where
    B: AsBundle,
{
    match B::key() {
        None => get_archetype_idx_with_idx_map(ids, archetypes, bundle),
        Some(key) => match keys.entry(key) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = get_archetype_idx_with_idx_map(ids, archetypes, bundle);
                entry.insert(idx);
                idx
            }
        },
    }
}

fn get_add_archetype_idx<B>(archetypes: &mut Vec<Archetype>, src: u32, bundle: &B) -> u32
where
    B: AsBundle,
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
                Archetype::new(archetypes[src as usize].infos().chain(infos))
            });
            archetypes.push(archetype);
            let idx = archetypes.len() - 1;
            idx as u32
        }
        Some(idx) => idx as u32,
    }
}

fn get_add_archetype_idx_with_idx_map<B>(
    map: &mut HashMap<(u32, Vec<TypeId>), u32, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
    bundle: &B,
) -> u32
where
    B: AsBundle,
{
    let raw_entry = bundle.with_ids(|ids| {
        let mut hasher = map.hasher().build_hasher();
        (src, ids).hash(&mut hasher);
        let hash = hasher.finish();

        map.raw_entry_mut()
            .from_hash(hash, |(key_src, key_ids)| *key_src == src && key_ids == ids)
    });

    match raw_entry {
        RawEntryMut::Occupied(entry) => *entry.get(),
        RawEntryMut::Vacant(entry) => {
            let idx = get_add_archetype_idx(archetypes, src, bundle);
            entry.insert(bundle.with_ids(|ids| (src, ids.into())), idx);
            idx
        }
    }
}

fn get_add_archetype_idx_with_maps<B>(
    keys: &mut HashMap<(u32, TypeId), u32, MulHasherBuilder>,
    ids: &mut HashMap<(u32, Vec<TypeId>), u32, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
    bundle: &B,
) -> u32
where
    B: AsBundle,
{
    match B::key() {
        None => get_add_archetype_idx_with_idx_map(ids, archetypes, src, bundle),
        Some(key) => match keys.entry((src, key)) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = get_add_archetype_idx_with_idx_map(ids, archetypes, src, bundle);
                entry.insert(idx);
                idx
            }
        },
    }
}

fn get_sub_archetype_idx<B>(archetypes: &mut Vec<Archetype>, src: u32, bundle: &B) -> u32
where
    B: AsBundle,
{
    let ids: Vec<_> = bundle.with_ids(|ids| {
        archetypes[src as usize]
            .ids()
            .filter(|id| ids.iter().all(|sub| sub != id))
            .collect()
    });

    match archetypes
        .iter()
        .position(|a| a.matches(ids.iter().copied()))
    {
        None => {
            assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

            let archetype = bundle.with_ids(|ids| {
                Archetype::new(
                    archetypes[src as usize]
                        .infos()
                        .filter(|info| ids.iter().all(|sub| *sub != info.id)),
                )
            });
            archetypes.push(archetype);
            let idx = archetypes.len() - 1;
            idx as u32
        }
        Some(idx) => idx as u32,
    }
}

fn get_sub_archetype_idx_with_idx_map<B>(
    map: &mut HashMap<(u32, Vec<TypeId>), u32, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
    bundle: &B,
) -> u32
where
    B: AsBundle,
{
    let raw_entry = bundle.with_ids(|ids| {
        let mut hasher = map.hasher().build_hasher();
        (src, ids).hash(&mut hasher);
        let hash = hasher.finish();

        map.raw_entry_mut()
            .from_hash(hash, |(key_src, key_ids)| *key_src == src && key_ids == ids)
    });

    match raw_entry {
        RawEntryMut::Occupied(entry) => *entry.get(),
        RawEntryMut::Vacant(entry) => {
            let idx = get_sub_archetype_idx(archetypes, src, bundle);
            entry.insert(bundle.with_ids(|ids| (src, ids.into())), idx);
            idx
        }
    }
}

fn get_sub_archetype_idx_with_maps<B>(
    keys: &mut HashMap<(u32, TypeId), u32, MulHasherBuilder>,
    ids: &mut HashMap<(u32, Vec<TypeId>), u32, MulHasherBuilder>,
    archetypes: &mut Vec<Archetype>,
    src: u32,
    bundle: &B,
) -> u32
where
    B: AsBundle,
{
    match B::key() {
        None => get_sub_archetype_idx_with_idx_map(ids, archetypes, src, bundle),
        Some(key) => match keys.entry((src, key)) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = get_sub_archetype_idx_with_idx_map(ids, archetypes, src, bundle);
                entry.insert(idx);
                idx
            }
        },
    }
}
