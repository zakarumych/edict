use alloc::vec::Vec;
use core::{
    any::TypeId,
    hash::{BuildHasher, Hash, Hasher},
};

use hashbrown::hash_map::{Entry, HashMap, RawEntryMut};

use crate::{
    archetype::Archetype,
    bundle::{Bundle, BundleDesc},
    cold,
    component::{ComponentInfo, ComponentRegistry},
    hash::{MulHasherBuilder, NoOpHasherBuilder},
};

use super::ArchetypeSet;

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
    #[must_use]
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

impl Edges {
    #[must_use]
    pub fn spawn<B, F>(
        &mut self,
        registry: &mut ComponentRegistry,
        archetypes: &mut ArchetypeSet,
        bundle: &B,
        register_components: F,
    ) -> u32
    where
        B: BundleDesc,
        F: FnOnce(&mut ComponentRegistry),
    {
        let very_slow = move || {
            cold();
            match archetypes
                .iter()
                .position(|a| bundle.with_ids(|ids| a.matches(ids.iter().copied())))
            {
                None => {
                    cold();

                    archetypes.add_with(|_| {
                        register_components(registry);

                        bundle.with_ids(|ids| {
                            Archetype::new(ids.iter().map(|id| match registry.get_info(*id) {
                                None => panic!("Component {:?} is not registered", id),
                                Some(info) => info,
                            }))
                        })
                    })
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

    #[must_use]
    pub fn insert<F>(
        &mut self,
        ty: TypeId,
        registry: &mut ComponentRegistry,
        archetypes: &mut ArchetypeSet,
        src: u32,
        register_component: F,
    ) -> u32
    where
        F: FnOnce(&mut ComponentRegistry) -> &ComponentInfo,
    {
        let slow = || {
            cold();
            match archetypes.iter().position(|a| {
                let ids = archetypes[src as usize].ids().chain(Some(ty));
                a.matches(ids)
            }) {
                None => {
                    cold();
                    archetypes.add_with(|archetypes| {
                        let info = register_component(registry);
                        Archetype::new(archetypes[src as usize].infos().chain(Some(info)))
                    })
                }
                Some(idx) => idx as u32,
            }
        };

        match self.add_one.entry((src, ty)) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = slow();
                entry.insert(idx);
                idx
            }
        }
    }

    #[must_use]
    pub fn insert_bundle<B, F>(
        &mut self,
        registry: &mut ComponentRegistry,
        archetypes: &mut ArchetypeSet,
        src: u32,
        bundle: &B,
        register_components: F,
    ) -> u32
    where
        B: BundleDesc,
        F: FnOnce(&mut ComponentRegistry),
    {
        let very_slow = || {
            cold();
            match archetypes.iter().position(|a| {
                bundle.with_ids(|ids| {
                    let ids = archetypes[src as usize].ids().chain(ids.iter().copied());
                    a.matches(ids)
                })
            }) {
                None => {
                    cold();
                    archetypes.add_with(|archetypes| {
                        register_components(registry);

                        bundle.with_ids(|ids| {
                            Archetype::new(
                                archetypes[src as usize]
                                    .ids()
                                    .filter(|aid| ids.iter().all(|id| *id != *aid))
                                    .chain(ids.iter().copied())
                                    .map(|id| match registry.get_info(id) {
                                        None => panic!("Component {:?} is not registered", id),
                                        Some(info) => info,
                                    }),
                            )
                        })
                    })
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

    #[must_use]
    pub fn remove(&mut self, archetypes: &mut ArchetypeSet, src: u32, ty: TypeId) -> u32 {
        let mut slow = || {
            cold();
            match archetypes.iter().position(|a| {
                let ids = archetypes[src as usize].ids().filter(|id| *id != ty);
                a.matches(ids)
            }) {
                None => {
                    cold();
                    archetypes.add_with(|archetypes| {
                        Archetype::new(
                            archetypes[src as usize]
                                .infos()
                                .filter(|info| info.id() != ty),
                        )
                    })
                }
                Some(idx) => idx as u32,
            }
        };

        match self.sub_one.entry((src, ty)) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let idx = slow();
                entry.insert(idx);
                idx
            }
        }
    }

    #[must_use]
    pub fn remove_bundle<B>(&mut self, archetypes: &mut ArchetypeSet, src: u32) -> u32
    where
        B: Bundle,
    {
        let mut very_slow = || {
            cold();
            let ids = archetypes[src as usize]
                .ids()
                .filter(|id| !B::static_contains_id(*id));

            match archetypes.iter().position(|a| a.matches(ids.clone())) {
                None => {
                    cold();
                    drop(ids);

                    archetypes.add_with(|archetypes| {
                        Archetype::new(
                            archetypes[src as usize]
                                .infos()
                                .filter(|info| !B::static_contains_id(info.id())),
                        )
                    })
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
