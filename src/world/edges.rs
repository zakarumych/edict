use core::{
    any::TypeId,
    hash::{BuildHasher, Hash, Hasher},
    marker::PhantomData,
};

use alloc::vec::Vec;

use hashbrown::hash_map::{Entry, HashMap, RawEntryMut};

use crate::{
    archetype::Archetype,
    bundle::{Bundle, DynamicBundle},
    component::Component,
    component::{ComponentInfo, ComponentRegistry},
    hash::{MulHasherBuilder, NoOpHasherBuilder},
    idx::MAX_IDX_USIZE,
};

pub(super) struct Edges {
    /// Maps static key of the bundle to archetype index.
    spawn_key: HashMap<TypeId, u32, NoOpHasherBuilder>,

    /// Maps ids list to archetype.
    spawn_ids: HashMap<Vec<TypeId>, u32, MulHasherBuilder>,

    /// Maps archetype index + additional component id to the archetype index.
    add_one: HashMap<(u32, TypeId), u32, MulHasherBuilder>,

    /// Maps archetype index + additional static bundle key to the archetype index.
    add_key: HashMap<(u32, TypeId), u32, MulHasherBuilder>,

    /// Maps archetype index + additional component ids list to archetype.
    add_ids: HashMap<(u32, Vec<TypeId>), u32, MulHasherBuilder>,

    /// Maps archetype index - removed component id to the archetype index.
    sub_one: HashMap<(u32, TypeId), u32, MulHasherBuilder>,

    /// Maps archetype index + removed static bundle key to the archetype index.
    sub_key: HashMap<(u32, TypeId), u32, MulHasherBuilder>,

    /// Maps archetype index + removed component ids list to archetype.
    sub_ids: HashMap<(u32, Vec<TypeId>), u32, MulHasherBuilder>,
}

impl Edges {
    pub fn new() -> Edges {
        Edges {
            spawn_key: HashMap::with_hasher(NoOpHasherBuilder),
            spawn_ids: HashMap::with_hasher(MulHasherBuilder),
            add_one: HashMap::with_hasher(MulHasherBuilder),
            add_key: HashMap::with_hasher(MulHasherBuilder),
            add_ids: HashMap::with_hasher(MulHasherBuilder),
            sub_one: HashMap::with_hasher(MulHasherBuilder),
            sub_key: HashMap::with_hasher(MulHasherBuilder),
            sub_ids: HashMap::with_hasher(MulHasherBuilder),
        }
    }
}

/// Umbrella trait for [`DynamicBundle`] and [`Bundle`].
pub(super) trait AsBundle {
    /// Returns static key if the bundle type have one.
    fn key() -> Option<TypeId>;

    /// Calls provided closure with slice of ids of types that this bundle contains.
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R;

    /// Calls provided closure with slice of component infos of types that this bundle contains.
    fn register_components(&self, registry: &mut ComponentRegistry);
}

impl<B> AsBundle for B
where
    B: DynamicBundle,
{
    #[inline]
    fn key() -> Option<TypeId> {
        <B as DynamicBundle>::key()
    }

    #[inline]
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
        DynamicBundle::with_ids(self, f)
    }

    #[inline]
    fn register_components(&self, registry: &mut ComponentRegistry) {
        DynamicBundle::with_components(self, |infos| {
            for info in infos {
                registry.register_erased(info.clone());
            }
        })
    }
}

impl<B> AsBundle for PhantomData<B>
where
    B: Bundle,
{
    #[inline]
    fn key() -> Option<TypeId> {
        Some(B::static_key())
    }

    #[inline]
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
        B::static_with_ids(f)
    }

    #[inline]
    fn register_components(&self, registry: &mut ComponentRegistry) {
        B::static_with_components(|infos| {
            for info in infos {
                registry.register_erased(info.clone());
            }
        })
    }
}

impl Edges {
    pub fn spawn<B>(
        &mut self,
        registry: &mut ComponentRegistry,
        archetypes: &mut Vec<Archetype>,
        bundle: &B,
    ) -> u32
    where
        B: AsBundle,
    {
        let mut very_slow = || {
            colder();
            match archetypes
                .iter()
                .position(|a| bundle.with_ids(|ids| a.matches(ids.iter().copied())))
            {
                None => {
                    coldest();
                    assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

                    bundle.register_components(registry);

                    let archetype = bundle.with_ids(|ids| {
                        Archetype::new(ids.iter().map(|id| registry.get_info(*id).unwrap()))
                    });

                    archetypes.push(archetype);

                    archetypes.len() as u32 - 1
                }
                Some(idx) => idx as u32,
            }
        };

        let spawn_ids = &mut self.spawn_ids;
        let slow = || {
            cold();
            let raw_entry = bundle.with_ids(move |ids| {
                let mut hasher = spawn_ids.hasher().build_hasher();
                ids.hash(&mut hasher);
                let hash = hasher.finish();

                spawn_ids
                    .raw_entry_mut()
                    .from_hash(hash, |key_ids| key_ids == ids)
            });

            match raw_entry {
                RawEntryMut::Occupied(entry) => *entry.get(),
                RawEntryMut::Vacant(entry) => {
                    let idx = very_slow();
                    entry.insert(bundle.with_ids(|ids| Vec::from(ids)), idx);
                    idx
                }
            }
        };

        match B::key() {
            None => slow(),
            Some(key) => match self.spawn_key.entry(key) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let idx = slow();
                    entry.insert(idx);
                    idx
                }
            },
        }
    }

    pub fn insert<T>(
        &mut self,
        registry: &mut ComponentRegistry,
        archetypes: &mut Vec<Archetype>,
        src: u32,
    ) -> u32
    where
        T: Component,
    {
        let mut slow = || {
            cold();
            match archetypes.iter().position(|a| {
                let ids = archetypes[src as usize]
                    .ids()
                    .chain(Some(TypeId::of::<T>()));
                a.matches(ids)
            }) {
                None => {
                    colder();
                    assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

                    registry.register::<T>();

                    let archetype = Archetype::new(
                        archetypes[src as usize]
                            .infos()
                            .chain(Some(&ComponentInfo::of::<T>())),
                    );

                    archetypes.push(archetype);
                    archetypes.len() as u32 - 1
                }
                Some(idx) => idx as u32,
            }
        };

        match self.add_one.entry((src, TypeId::of::<T>())) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = slow();
                entry.insert(idx);
                idx
            }
        }
    }

    pub fn insert_bundle<B>(
        &mut self,
        registry: &mut ComponentRegistry,
        archetypes: &mut Vec<Archetype>,
        src: u32,
        bundle: &B,
    ) -> u32
    where
        B: AsBundle,
    {
        let mut very_slow = || {
            colder();
            match archetypes.iter().position(|a| {
                bundle.with_ids(|ids| {
                    let ids = archetypes[src as usize].ids().chain(ids.iter().copied());
                    a.matches(ids)
                })
            }) {
                None => {
                    coldest();
                    assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

                    bundle.register_components(registry);

                    let archetype = bundle.with_ids(|ids| {
                        Archetype::new(
                            archetypes[src as usize]
                                .ids()
                                .filter(|aid| ids.iter().all(|id| *id != *aid))
                                .chain(ids.iter().copied())
                                .map(|id| registry.get_info(id).unwrap()),
                        )
                    });

                    archetypes.push(archetype);

                    archetypes.len() as u32 - 1
                }
                Some(idx) => idx as u32,
            }
        };

        let add_ids = &mut self.add_ids;
        let slow = || {
            cold();
            let raw_entry = bundle.with_ids(move |ids| {
                let mut hasher = add_ids.hasher().build_hasher();
                (src, ids).hash(&mut hasher);
                let hash = hasher.finish();

                add_ids
                    .raw_entry_mut()
                    .from_hash(hash, |(key_src, key_ids)| *key_src == src && key_ids == ids)
            });

            match raw_entry {
                RawEntryMut::Occupied(entry) => *entry.get(),
                RawEntryMut::Vacant(entry) => {
                    let idx = very_slow();
                    entry.insert((src, bundle.with_ids(|ids| Vec::from(ids))), idx);
                    idx
                }
            }
        };

        match B::key() {
            None => slow(),
            Some(key) => match self.add_key.entry((src, key)) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let idx = slow();
                    entry.insert(idx);
                    idx
                }
            },
        }
    }

    pub fn remove(&mut self, archetypes: &mut Vec<Archetype>, src: u32, id: TypeId) -> u32 {
        let mut slow = || {
            cold();
            match archetypes.iter().position(|a| {
                let ids = archetypes[src as usize].ids().filter(|t| *t != id);
                a.matches(ids)
            }) {
                None => {
                    colder();
                    assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

                    let archetype = Archetype::new(
                        archetypes[src as usize]
                            .infos()
                            .filter(|info| info.id() != id),
                    );

                    // let meta = RemoveMeta::new::<T>(&archetypes[src as usize], &archetype);

                    archetypes.push(archetype);

                    archetypes.len() as u32 - 1
                }
                Some(idx) => idx as u32,
            }
        };

        match self.sub_one.entry((src, id)) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = slow();
                entry.insert(idx);
                idx
            }
        }
    }

    pub fn remove_bundle<B>(&mut self, archetypes: &mut Vec<Archetype>, src: u32) -> u32
    where
        B: Bundle,
    {
        let mut very_slow = || {
            colder();
            let ids = archetypes[src as usize]
                .ids()
                .filter(|id| !B::static_contains_id(*id));

            match archetypes.iter().position(|a| a.matches(ids.clone())) {
                None => {
                    coldest();
                    drop(ids);

                    assert!(archetypes.len() < MAX_IDX_USIZE, "Too many archetypes");

                    let archetype = Archetype::new(
                        archetypes[src as usize]
                            .infos()
                            .filter(|info| !B::static_contains_id(info.id())),
                    );

                    archetypes.push(archetype);

                    archetypes.len() as u32 - 1
                }
                Some(idx) => idx as u32,
            }
        };

        let sub_ids = &mut self.sub_ids;
        let slow = || {
            cold();
            let raw_entry = B::static_with_ids(move |ids| {
                let mut hasher = sub_ids.hasher().build_hasher();
                (src, ids).hash(&mut hasher);
                let hash = hasher.finish();

                sub_ids
                    .raw_entry_mut()
                    .from_hash(hash, |(key_src, key_ids)| *key_src == src && key_ids == ids)
            });

            match raw_entry {
                RawEntryMut::Occupied(entry) => *entry.get(),
                RawEntryMut::Vacant(entry) => {
                    let idx = very_slow();
                    entry.insert((src, B::static_with_ids(|ids| ids.into())), idx);
                    idx
                }
            }
        };

        match B::key() {
            None => slow(),
            Some(key) => match self.sub_key.entry((src, key)) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let idx = slow();
                    entry.insert(idx);
                    idx
                }
            },
        }
    }
}

#[cold]
#[inline(always)]
fn cold() {}

#[cold]
#[inline(always)]
fn colder() {
    cold();
}

#[cold]
#[inline(always)]
fn coldest() {
    colder();
}
