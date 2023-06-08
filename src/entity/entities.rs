use core::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use hashbrown::{hash_map::Entry, HashMap};

use crate::world::NoSuchEntity;

use super::{
    allocator::{IdAllocator, IdRangeAllocator},
    EntityId,
};

/// Entity location in archetypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    /// Archetype index.
    archetype: u32,

    /// Index within archetype.
    idx: u32,
}

impl Location {
    fn reserved(idx: u32) -> Self {
        Location {
            archetype: u32::MAX,
            idx,
        }
    }
}

pub struct EntitySet {
    map: HashMap<u64, Location>,
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

    #[inline(always)]
    pub fn alloc_mut(&mut self) -> EntityId {
        match self.id_allocator.next() {
            None => {
                panic!("Entity id allocator is exhausted");
            }
            Some(id) => EntityId::new(id),
        }
    }

    #[inline(always)]
    pub fn spawn(&mut self) -> EntityId {
        let id = self.alloc_mut();
        self.spawn_at(id);
        id
    }

    #[inline(always)]
    pub fn spawn_at(&mut self, id: EntityId) {
        let old = self.map.insert(
            id.bits(),
            Location {
                archetype: 0,
                idx: 0,
            },
        );
        debug_assert!(old.is_none());
    }

    #[inline(always)]
    pub fn spawn_if_missing(&mut self, id: EntityId) -> bool {
        match self.map.entry(id.bits()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(Location {
                    archetype: 0,
                    idx: 0,
                });
                true
            }
        }
    }

    #[inline(always)]
    pub fn alloc(&self) -> EntityId {
        let idx = self.reserve_counter.fetch_add(1, Ordering::Relaxed);

        if idx >= u32::MAX as u64 {
            self.reserve_counter.fetch_sub(1, Ordering::Relaxed);
            panic!("Too much entity ids reserved");
        }

        match self.id_allocator.reserve(idx) {
            None => {
                self.reserve_counter.fetch_sub(1, Ordering::Relaxed);
                panic!("Too much entity ids reserved");
            }
            Some(id) => EntityId::new(id),
        }
    }

    #[inline(always)]
    pub fn spawn_allocated(&mut self, mut f: impl FnMut(EntityId) -> u32) {
        let reserved = core::mem::replace(self.reserve_counter.get_mut(), 0);
        if reserved == 0 {
            return;
        }
        unsafe {
            self.id_allocator.flush_reserved(reserved, |id| {
                self.map.insert(
                    id.get(),
                    Location {
                        archetype: 0,
                        idx: f(EntityId::new(id)),
                    },
                );
            });
        }
    }

    #[inline(always)]
    pub fn despawn(&mut self, id: EntityId) -> Result<Location, NoSuchEntity> {
        match self.map.remove(&id.bits()) {
            None => Err(NoSuchEntity),
            Some(loc) => Ok(loc),
        }
    }

    #[inline(always)]
    pub fn set_location(&mut self, id: EntityId, loc: Location) {
        self.map[&id.bits()] = loc;
    }

    #[inline(always)]
    pub fn get_location(&self, id: EntityId) -> Option<Location> {
        match self.map.get(&id.bits()) {
            None => {
                let reserved = self.reserve_counter.load(Ordering::Acquire);
                let Some(idx) = self.id_allocator.reserved(id.value()) else {
                    return None
                };
                if idx >= reserved {
                    return None;
                }
                Some(Location::reserved(idx as u32))
            }
            Some(loc) => Some(*loc),
        }
    }

    pub fn reserve_space(&mut self, additional: usize) {
        self.map.reserve(additional);
    }
}
