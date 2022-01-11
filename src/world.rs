use core::{
    any::{type_name, TypeId},
    hash::{BuildHasher, Hash, Hasher},
    marker::PhantomData,
    num::NonZeroU32,
};

use alloc::{sync::Arc, vec::Vec};
use hashbrown::{
    hash_map::{Entry, RawEntryMut},
    HashMap,
};

use crate::{
    archetype::{self, split_idx, Archetype},
    bundle::{Bundle, DynamicBundle},
    component::{Component, ComponentInfo},
    entity::{DropQueue, Entity, WeakEntity},
    idx::MAX_IDX_USIZE,
    proof::Proof,
    query::{Fetch, ImmutableQuery, NonTrackingQuery, Query, QueryMut, QueryTrackedMut},
    tracks::Tracks,
};

fn invalid_gen() -> NonZeroU32 {
    NonZeroU32::new(1).unwrap()
}

impl WeakEntity {
    /// Returns weak entity instance that is already expired.
    pub fn dangling() -> Self {
        WeakEntity::new(0, invalid_gen())
    }
}

fn first_gen() -> NonZeroU32 {
    NonZeroU32::new(2).unwrap()
}

/// Stores entity information in the World
#[derive(Debug)]
struct EntityData {
    /// Entity generation.
    gen: u32,

    /// Archetype index.
    archetype: u32,

    /// Index within archetype.
    idx: u32,
}

/// Entities container.
#[derive(Debug)]
pub struct World {
    /// Global epoch counter of the World.
    /// Incremented on each mutable query.
    epoch: u64,

    /// Array of all entities in the world.
    entities: Vec<EntityData>,

    /// Array of indices of free entity data slots.
    /// Does not include ids with exhausted generation index.
    free_entity_ids: Vec<u32>,

    /// Archetypes of entities in the world.
    archetypes: Vec<Archetype>,

    /// Maps static key of the bundle to archetype index.
    keys: HashMap<TypeId, u32>,

    /// Maps ids list to archetype.
    ids: HashMap<Vec<TypeId>, u32>,

    /// Maps archetype index + additional static bundle key to the archetype index.
    add_key: HashMap<(u32, TypeId), u32>,

    /// Maps archetype index + ids list to archetype.
    add_ids: HashMap<(u32, Vec<TypeId>), u32>,

    /// Maps archetype index - additional component id to the archetype index.
    sub_key: HashMap<(u32, TypeId), u32>,

    /// Maps archetype index - additional component id to the archetype index.
    sub_ids: HashMap<(u32, Vec<TypeId>), u32>,

    /// Queue of dropped entities.
    drop_queue: DropQueue,
}

impl World {
    pub fn new() -> Self {
        World {
            epoch: 0,
            entities: Vec::new(),
            free_entity_ids: Vec::new(),
            archetypes: Vec::new(),
            keys: HashMap::new(),
            ids: HashMap::new(),
            add_key: HashMap::new(),
            add_ids: HashMap::new(),
            sub_key: HashMap::new(),
            sub_ids: HashMap::new(),

            drop_queue: DropQueue::new(0),
        }
    }

    pub fn spawn<B>(&mut self, bundle: B) -> Entity
    where
        B: DynamicBundle,
    {
        let id = match self.free_entity_ids.pop() {
            None => {
                let id = self.entities.len();
                if id == MAX_IDX_USIZE {
                    panic!("Too many entities");
                }

                id as u32
            }
            Some(id) => id,
        };

        let archetype_idx = get_archetype_idx_with_maps(
            &mut self.keys,
            &mut self.ids,
            &mut self.archetypes,
            &bundle,
        );

        self.epoch += 1;
        let idx = self.archetypes[archetype_idx as usize].spawn(id, bundle, self.epoch);

        let entity = if id == self.entities.len() as u32 {
            self.entities.push(EntityData {
                gen: 2,
                idx,
                archetype: archetype_idx,
            });

            Entity::new(id, first_gen(), &self.drop_queue)
        } else {
            let data = &mut self.entities[id as usize];
            let gen = NonZeroU32::new(data.gen).unwrap();

            Entity::new(id, gen, &self.drop_queue)
        };

        entity
    }

    pub fn pin<B>(&mut self, entity: Entity) -> Entity<B>
    where
        B: Bundle,
    {
        debug_assert!(entity.is_from_queue(&self.drop_queue));

        let data = &self.entities[entity.id as usize];
        assert_eq!(data.gen, entity.gen.get());

        let archetype = &self.archetypes[data.archetype as usize];

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
    pub fn get<'a, Q, A: 'a>(&'a self, entity: &Entity<A>) -> <Q::Fetch as Fetch<'a>>::Item
    where
        &'a A: Proof<Q>,
        Q: Query + ImmutableQuery + NonTrackingQuery,
    {
        debug_assert!(entity.is_from_queue(&self.drop_queue));

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

        let data = &self.entities[entity.id as usize];
        assert_eq!(data.gen, entity.gen.get());
        let archetype = &self.archetypes[data.archetype as usize];
        let mut fetch = unsafe { Q::fetch(archetype, 0, self.epoch) }.expect("Query is prooven");

        let (chunk_idx, entity_idx) = split_idx(data.idx);

        let mut chunk = unsafe { fetch.get_chunk(chunk_idx) };
        let item = unsafe { Q::Fetch::get_item(&mut chunk, entity_idx) };
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
    pub fn get_mut<'a, Q, A: 'a>(&'a mut self, entity: &Entity<A>) -> <Q::Fetch as Fetch<'a>>::Item
    where
        &'a mut A: Proof<Q>,
        Q: Query + NonTrackingQuery,
    {
        debug_assert!(entity.is_from_queue(&self.drop_queue));

        assert!(
            !Q::tracks(),
            "Invalid impl of `NonTrackingQuery` for `{}`",
            type_name::<Q>()
        );

        if Q::mutates() {
            self.epoch += 1;
        }

        let data = &self.entities[entity.id as usize];
        assert_eq!(data.gen, entity.gen.get());
        let archetype = &self.archetypes[data.archetype as usize];
        let mut fetch = unsafe { Q::fetch(archetype, 0, self.epoch) }.expect("Query is prooven");

        let (chunk_idx, entity_idx) = split_idx(data.idx);

        let mut chunk = unsafe { fetch.get_chunk(chunk_idx) };
        let item = unsafe { Q::Fetch::get_item(&mut chunk, entity_idx) };
        item
    }

    /// Queries components from specified entity.
    ///
    /// If query cannot be satisfied, returns `EntityError::MissingComponents`.
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

        if self.entities.len() as u32 <= entity.id {
            return Err(EntityError::NoSuchEntity);
        }

        let data = &self.entities[entity.id as usize];
        if data.gen != entity.gen.get() {
            return Err(EntityError::NoSuchEntity);
        }
        let archetype = &self.archetypes[data.archetype as usize];
        match unsafe { Q::fetch(archetype, 0, self.epoch) } {
            None => Err(EntityError::MissingComponents),
            Some(mut fetch) => {
                let (chunk_idx, entity_idx) = split_idx(data.idx);

                let mut chunk = unsafe { fetch.get_chunk(chunk_idx) };
                let item = unsafe { Q::Fetch::get_item(&mut chunk, entity_idx) };
                Ok(item)
            }
        }
    }

    /// Queries components from specified entity.
    ///
    /// If query cannot be satisfied, returns `EntityError::MissingComponents`.
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

        if self.entities.len() as u32 <= entity.id {
            return Err(EntityError::NoSuchEntity);
        }

        let data = &self.entities[entity.id as usize];
        if data.gen != entity.gen.get() {
            return Err(EntityError::NoSuchEntity);
        }
        let archetype = &self.archetypes[data.archetype as usize];
        match unsafe { Q::fetch(archetype, 0, self.epoch) } {
            None => Err(EntityError::MissingComponents),
            Some(mut fetch) => {
                let (chunk_idx, entity_idx) = split_idx(data.idx);

                let mut chunk = unsafe { fetch.get_chunk(chunk_idx) };
                let item = unsafe { Q::Fetch::get_item(&mut chunk, entity_idx) };
                Ok(item)
            }
        }
    }

    pub fn query_mut<'a, Q>(&'a mut self) -> QueryMut<'a, Q>
    where
        Q: Query + NonTrackingQuery,
    {
        if Q::mutates() {
            self.epoch += 1
        };

        QueryMut {
            epoch: self.epoch,
            archetypes: &self.archetypes,
            query: PhantomData,
        }
    }

    pub fn query_tracked_mut<'a, Q>(&'a mut self, tracks: &mut Tracks) -> QueryTrackedMut<'a, Q>
    where
        Q: Query,
    {
        if Q::mutates() {
            self.epoch += 1
        };

        QueryTrackedMut {
            tracks: tracks.epoch,
            epoch: {
                tracks.epoch = self.epoch;
                self.epoch
            },
            archetypes: &self.archetypes,
            query: PhantomData,
        }
    }

    pub fn tracks(&self) -> Tracks {
        Tracks { epoch: self.epoch }
    }

    pub fn maintain(&mut self) {
        for id in self.drop_queue.drain() {
            let data = &self.entities[id as usize];
            let idx = data.idx;
            let gen = data.gen;

            let archetype = &mut self.archetypes[data.archetype as usize];
            let opt_id = unsafe { archetype.despawn(idx) };

            if let Some(id) = opt_id {
                self.entities[id as usize].idx = idx;
            }

            if gen == u32::MAX {
                // Exhausted.
                self.entities[id as usize].gen = 0;
            } else {
                self.entities[id as usize].gen += 1;
                self.free_entity_ids.push(id);
            }
        }
    }

    pub fn insert<T>(&mut self, entity: &WeakEntity, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        if self.entities.len() as u32 <= entity.id {
            return Err(NoSuchEntity);
        }

        let data = &mut self.entities[entity.id as usize];
        if data.gen != entity.gen.get() {
            return Err(NoSuchEntity);
        }

        self.epoch += 1;

        if self.archetypes[data.archetype as usize].contains_id(TypeId::of::<T>()) {
            unsafe {
                self.archetypes[data.archetype as usize].set(data.idx, component, self.epoch);
            }

            return Ok(());
        }

        let archetype_idx = get_add_archetype_idx_with_maps(
            &mut self.add_key,
            &mut self.add_ids,
            &mut self.archetypes,
            data.archetype,
            &PhantomData::<(T,)>,
        );

        debug_assert_ne!(data.archetype, archetype_idx);

        let (before, after) = self
            .archetypes
            .split_at_mut(data.archetype.max(archetype_idx) as usize);

        let (src, dst) = match data.archetype < archetype_idx {
            true => (&mut before[data.archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[archetype_idx as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.insert(dst, data.idx, component, self.epoch) };

        let old_idx = data.idx;

        data.idx = dst_idx;
        data.archetype = archetype_idx;

        if let Some(src_id) = opt_src_id {
            self.entities[src_id as usize].idx = old_idx;
        }

        Ok(())
    }

    pub fn remove<T>(&mut self, entity: &WeakEntity) -> Result<T, EntityError>
    where
        T: Component,
    {
        if self.entities.len() as u32 <= entity.id {
            return Err(EntityError::NoSuchEntity);
        }

        let data = &mut self.entities[entity.id as usize];
        if data.gen != entity.gen.get() {
            return Err(EntityError::NoSuchEntity);
        }

        self.epoch += 1;

        if !self.archetypes[data.archetype as usize].contains_id(TypeId::of::<T>()) {
            return Err(EntityError::MissingComponents);
        }

        let archetype_idx = get_sub_archetype_idx_with_maps(
            &mut self.sub_key,
            &mut self.sub_ids,
            &mut self.archetypes,
            data.archetype,
            &PhantomData::<(T,)>,
        );

        debug_assert_ne!(data.archetype, archetype_idx);

        let (before, after) = self
            .archetypes
            .split_at_mut(data.archetype.max(archetype_idx) as usize);

        let (src, dst) = match data.archetype < archetype_idx {
            true => (&mut before[data.archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[archetype_idx as usize]),
        };

        let (dst_idx, opt_src_id, component) = unsafe { src.remove(dst, data.idx) };

        let old_idx = data.idx;

        data.idx = dst_idx;
        data.archetype = archetype_idx;

        if let Some(src_id) = opt_src_id {
            self.entities[src_id as usize].idx = old_idx;
        }

        Ok(component)
    }

    pub fn insert_bundle<B>(&mut self, entity: &WeakEntity, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        if self.entities.len() as u32 <= entity.id {
            return Err(NoSuchEntity);
        }

        let data = &mut self.entities[entity.id as usize];
        if data.gen != entity.gen.get() {
            return Err(NoSuchEntity);
        }

        if bundle.with_ids(|ids| ids.is_empty()) {
            return Ok(());
        }

        self.epoch += 1;

        let archetype_idx = get_add_archetype_idx_with_maps(
            &mut self.add_key,
            &mut self.add_ids,
            &mut self.archetypes,
            data.archetype,
            &bundle,
        );

        if archetype_idx == data.archetype {
            unsafe {
                self.archetypes[data.archetype as usize].set_bundle(data.idx, bundle, self.epoch)
            };
            return Ok(());
        }

        let (before, after) = self
            .archetypes
            .split_at_mut(data.archetype.max(archetype_idx) as usize);

        let (src, dst) = match data.archetype < archetype_idx {
            true => (&mut before[data.archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[data.archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.insert_bundle(dst, data.idx, bundle, self.epoch) };

        let old_idx = data.idx;

        data.idx = dst_idx;
        data.archetype = archetype_idx;

        if let Some(src_id) = opt_src_id {
            self.entities[src_id as usize].idx = old_idx;
        }

        Ok(())
    }

    pub fn remove_bundle<B>(&mut self, entity: &WeakEntity) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        if self.entities.len() as u32 <= entity.id {
            return Err(NoSuchEntity);
        }

        let data = &mut self.entities[entity.id as usize];
        if data.gen != entity.gen.get() {
            return Err(NoSuchEntity);
        }

        if B::static_with_ids(|ids| ids.is_empty()) {
            return Ok(());
        }

        self.epoch += 1;

        let archetype_idx = get_sub_archetype_idx_with_maps(
            &mut self.sub_key,
            &mut self.sub_ids,
            &mut self.archetypes,
            data.archetype,
            &PhantomData::<B>,
        );

        debug_assert_ne!(data.archetype, archetype_idx);

        let (before, after) = self
            .archetypes
            .split_at_mut(data.archetype.max(archetype_idx) as usize);

        let (src, dst) = match data.archetype < archetype_idx {
            true => (&mut before[data.archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[data.archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.drop_bundle(dst, data.idx) };

        let old_idx = data.idx;

        data.idx = dst_idx;
        data.archetype = archetype_idx;

        if let Some(src_id) = opt_src_id {
            self.entities[src_id as usize].idx = old_idx;
        }

        Ok(())
    }
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
    map: &mut HashMap<Vec<TypeId>, u32>,
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
    keys: &mut HashMap<TypeId, u32>,
    ids: &mut HashMap<Vec<TypeId>, u32>,
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
    map: &mut HashMap<(u32, Vec<TypeId>), u32>,
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
    keys: &mut HashMap<(u32, TypeId), u32>,
    ids: &mut HashMap<(u32, Vec<TypeId>), u32>,
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
    map: &mut HashMap<(u32, Vec<TypeId>), u32>,
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
    keys: &mut HashMap<(u32, TypeId), u32>,
    ids: &mut HashMap<(u32, Vec<TypeId>), u32>,
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
