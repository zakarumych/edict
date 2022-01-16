use core::{
    any::{type_name, TypeId},
    hash::{BuildHasher, Hash, Hasher},
    marker::PhantomData,
};

use alloc::vec::Vec;
use hashbrown::{
    hash_map::{Entry, RawEntryMut},
    HashMap,
};

use crate::{
    archetype::{split_idx, Archetype},
    bundle::{Bundle, DynamicBundle},
    component::{Component, ComponentInfo},
    entity::{Entities, Entity, WeakEntity},
    idx::MAX_IDX_USIZE,
    proof::Proof,
    query::{Fetch, ImmutableQuery, NonTrackingQuery, Query, QueryIter, QueryTrackedIter},
    tracks::Tracks,
};

/// Entities container.
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
}

impl World {
    pub fn new() -> Self {
        World {
            epoch: 0,
            entities: Entities::new(1024),
            archetypes: Vec::new(),
            keys: HashMap::new(),
            ids: HashMap::new(),
            add_key: HashMap::new(),
            add_ids: HashMap::new(),
            sub_key: HashMap::new(),
            sub_ids: HashMap::new(),
        }
    }

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
        let idx = self.archetypes[archetype_idx as usize].spawn(*entity, bundle, self.epoch);
        self.entities.set_location(entity.id, archetype_idx, idx);
        entity
    }

    pub fn insert<T, P>(&mut self, entity: &Entity<P>, component: T)
    where
        T: Component,
    {
        assert!(self.entities.is_owner_of(entity));

        self.try_insert(entity, component).expect("Entity exists");
    }

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

    pub fn insert_bundle<B, T>(&mut self, entity: &Entity<T>, bundle: B)
    where
        B: DynamicBundle,
    {
        assert!(self.entities.is_owner_of(entity));
        self.try_insert_bundle(entity, bundle).unwrap();
    }

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

        let (chunk_idx, entity_idx) = split_idx(idx);

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

        let (chunk_idx, entity_idx) = split_idx(idx);

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

        let (archetype, idx) = self.entities.get(entity).ok_or(EntityError::NoSuchEntity)?;
        let archetype = &self.archetypes[archetype as usize];
        match unsafe { Q::fetch(archetype, 0, self.epoch) } {
            None => Err(EntityError::MissingComponents),
            Some(mut fetch) => {
                let (chunk_idx, entity_idx) = split_idx(idx);

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

        let (archetype, idx) = self.entities.get(entity).ok_or(EntityError::NoSuchEntity)?;
        let archetype = &self.archetypes[archetype as usize];
        match unsafe { Q::fetch(archetype, 0, self.epoch) } {
            None => Err(EntityError::MissingComponents),
            Some(mut fetch) => {
                let (chunk_idx, entity_idx) = split_idx(idx);

                let mut chunk = unsafe { fetch.get_chunk(chunk_idx) };
                let item = unsafe { Q::Fetch::get_item(&mut chunk, entity_idx) };
                Ok(item)
            }
        }
    }

    pub fn query<'a, Q>(&'a self) -> QueryIter<'a, Q>
    where
        Q: Query + NonTrackingQuery + ImmutableQuery,
    {
        QueryIter::new(self.epoch, &self.archetypes)
    }

    pub fn query_tracked<'a, Q>(&'a self, tracks: &mut Tracks) -> QueryTrackedIter<'a, Q>
    where
        Q: Query + ImmutableQuery,
    {
        let iter = QueryTrackedIter::new(tracks.epoch, self.epoch, &self.archetypes);
        tracks.epoch = self.epoch;
        iter
    }

    pub fn query_mut<'a, Q>(&'a mut self) -> QueryIter<'a, Q>
    where
        Q: Query + NonTrackingQuery,
    {
        if Q::mutates() {
            self.epoch += 1
        };

        QueryIter::new(self.epoch, &self.archetypes)
    }

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

    pub fn has_component<T: 'static, U>(&self, entity: &Entity<U>) -> bool {
        assert!(self.entities.is_owner_of(entity));

        let (archetype, _idx) = self.entities.get(entity).unwrap();
        self.archetypes[archetype as usize].contains_id(TypeId::of::<T>())
    }

    pub fn has_component_weak<T: 'static>(
        &self,
        entity: &WeakEntity,
    ) -> Result<bool, NoSuchEntity> {
        let (archetype, _idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;
        Ok(self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()))
    }

    pub fn is_alive(&self, entity: &WeakEntity) -> bool {
        self.entities.get(entity).is_some()
    }

    pub fn tracks(&self) -> Tracks {
        Tracks { epoch: self.epoch }
    }

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
