use core::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use alloc::boxed::Box;

use hashbrown::{hash_map::Entry, HashMap};

use crate::world::NoSuchEntity;

use super::{
    allocator::{IdAllocator, IdRangeAllocator},
    EntityId,
};

/// Stores entity information in the World
struct EntityData {
    /// Archetype index.
    archetype: u32,

    /// Index within archetype.
    idx: u32,
}

impl fmt::Debug for EntityData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityData")
            .field("archetype", &self.archetype)
            .field("idx", &self.idx)
            .finish()
    }
}

pub(crate) struct EntitySet {
    map: HashMap<u64, EntityData>,
    id_allocator: IdAllocator,
    reserve_counter: AtomicU64,
}

impl fmt::Debug for EntitySet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entities")
            .field("entities", &self.map)
            .finish_non_exhaustive()
    }
}

impl EntitySet {
    pub fn new() -> Self {
        EntitySet {
            map: HashMap::new(),
            id_allocator: IdAllocator::new(),
            reserve_counter: AtomicU64::new(0),
        }
    }

    pub fn with_allocator(id_allocator: Box<dyn IdRangeAllocator>) -> Self {
        EntitySet {
            map: HashMap::new(),
            id_allocator: IdAllocator::with_range_allocator(id_allocator),
            reserve_counter: AtomicU64::new(0),
        }
    }

    pub fn alloc_mut(&mut self) -> EntityId {
        match self.id_allocator.next() {
            None => {
                panic!("Entity id allocator is exhausted");
            }
            Some(id) => EntityId::new(id),
        }
    }

    pub fn spawn(&mut self) -> EntityId {
        let id = self.alloc_mut();
        self.spawn_at(id);
        id
    }

    pub fn spawn_at(&mut self, id: EntityId) {
        let old = self.map.insert(
            id.bits(),
            EntityData {
                archetype: 0,
                idx: 0,
            },
        );
        debug_assert!(old.is_none());
    }

    pub fn spawn_if_missing(&mut self, id: EntityId) -> bool {
        match self.map.entry(id.bits()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(EntityData {
                    archetype: 0,
                    idx: 0,
                });
                true
            }
        }
    }

    pub fn alloc(&self) -> EntityId {
        let idx = self.reserve_counter.fetch_add(1, Ordering::Relaxed);

        match self.id_allocator.reserve(idx) {
            None => {
                self.reserve_counter.fetch_sub(1, Ordering::Relaxed);
                panic!("Too much entity ids reserved");
            }
            Some(id) => EntityId::new(id),
        }
    }

    pub fn spawn_allocated(&mut self, mut f: impl FnMut(EntityId) -> u32) {
        let reserved = core::mem::replace(self.reserve_counter.get_mut(), 0);
        unsafe {
            self.id_allocator.flush_reserved(reserved, |id| {
                self.map.insert(
                    id.get(),
                    EntityData {
                        archetype: 0,
                        idx: f(EntityId::new(id)),
                    },
                );
            });
        }
    }

    pub fn despawn(&mut self, id: EntityId) -> Result<(u32, u32), NoSuchEntity> {
        match self.map.remove(&id.bits()) {
            None => Err(NoSuchEntity),
            Some(data) => Ok((data.archetype, data.idx)),
        }
    }

    pub fn set_location(&mut self, id: EntityId, archetype: u32, idx: u32) {
        let data = self.map.get_mut(&id.bits()).expect("Invalid entity id");
        data.archetype = archetype;
        data.idx = idx;
    }

    pub fn get_location(&self, id: EntityId) -> Option<(u32, u32)> {
        match self.map.get(&id.bits()) {
            None => {
                let bits = id.bits();
                let reserved = self.reserve_counter.load(Ordering::Acquire);
                let Some(idx) = self.id_allocator.reserved(bits) else {
                    return None
                };
                if idx >= reserved {
                    return None;
                }

                Some((u32::MAX, 0))
            }
            Some(data) => Some((data.archetype, data.idx)),
        }
    }

    pub fn reserve_space(&mut self, additional: usize) {
        self.map.reserve(additional);
    }
}
