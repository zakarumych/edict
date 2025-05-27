use alloc::boxed::Box;
use core::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use hashbrown::{hash_map::Entry, HashMap};

use crate::cold;

use super::{
    allocator::{IdAllocator, IdRangeAllocator},
    EntityId, EntityLoc,
};

/// Entity location in archetypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    /// Archetype index.
    pub arch: u32,

    /// Index within archetype.
    pub idx: u32,
}

impl Location {
    /// Creates a new location instance.
    pub fn new(arch_idx: u32, idx: u32) -> Self {
        Location {
            arch: arch_idx,
            idx,
        }
    }

    /// Creates a new location instance with empty archetype index.
    pub fn empty(idx: u32) -> Self {
        Location { arch: 0, idx }
    }

    /// Creates a new location instance with reserved archetype index.
    pub fn reserved(idx: u32) -> Self {
        Location {
            arch: u32::MAX,
            idx,
        }
    }
}

/// Collection of entities with mapping to their location.
///
/// User typically interacts with `World` that contains `EntitySet` internally.
/// When API needs `EntitySet`, `World::entities()` method is used to get it.
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
    /// Creates a new entity set.
    pub fn new() -> Self {
        EntitySet {
            map: HashMap::new(),
            id_allocator: IdAllocator::new(),
            reserve_counter: AtomicU64::new(0),
        }
    }

    /// Creates a new entity set with custom ID range allocator.
    ///
    /// Custom range allocator is typically used to create
    /// entity sets that allocate IDs without collisions.
    pub fn with_allocator(id_allocator: Box<dyn IdRangeAllocator>) -> Self {
        EntitySet {
            map: HashMap::new(),
            id_allocator: IdAllocator::with_range_allocator(id_allocator),
            reserve_counter: AtomicU64::new(0),
        }
    }

    /// Spawns entity with new ID in specified archetype.
    /// Calls provided closure to place entity into archetype and acquire index.
    /// Returns entity ID and location.
    #[inline]
    pub fn spawn(&mut self, arch: u32, f: impl FnOnce(EntityId) -> u32) -> (EntityId, Location) {
        let Some(id) = self.id_allocator.next() else {
            panic!("Entity id allocator is exhausted");
        };
        let id = EntityId::new(id);
        let loc = Location { arch, idx: f(id) };
        let old = self.map.insert(id.bits(), loc);
        debug_assert!(old.is_none());
        (id, loc)
    }

    /// Spawns entity with specified ID in specified archetype.
    /// If entity with specified ID already exists, returns `false` and its location.
    /// Otherwise calls provided closure to place entity into archetype and acquire index.
    /// And then returns `true` and entity location.
    #[inline]
    pub fn spawn_at(
        &mut self,
        id: EntityId,
        arch: u32,
        f: impl FnOnce() -> u32,
    ) -> (bool, Location) {
        match self.map.entry(id.bits()) {
            Entry::Occupied(entry) => (false, *entry.get()),
            Entry::Vacant(entry) => {
                let loc = Location { arch, idx: f() };
                entry.insert(loc);
                (true, loc)
            }
        }
    }

    /// Allocate entity ID without spawning.
    /// Entity is place into "reserved" archetype at index `u32::MAX`.
    /// Allocated entities must be spawned with `spawn_allocated`
    /// before any other entity is spawned.
    #[inline]
    pub fn alloc(&self) -> EntityLoc<'_> {
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
            Some(id) => EntityLoc::from_parts(EntityId::new(id), Location::reserved(idx as u32)),
        }
    }

    /// Spawns all allocated entities.
    #[inline]
    pub fn spawn_allocated(&mut self, mut f: impl FnMut(EntityId) -> u32) {
        let reserved = core::mem::replace(self.reserve_counter.get_mut(), 0);
        if reserved == 0 {
            return;
        }

        cold();
        unsafe {
            self.id_allocator.flush_reserved(reserved, |id| {
                self.map.insert(
                    id.get(),
                    Location {
                        arch: 0,
                        idx: f(EntityId::new(id)),
                    },
                );
            });
        }
    }

    /// Despawns entity with specified ID.
    #[inline]
    pub fn despawn(&mut self, id: EntityId) -> Option<Location> {
        self.map.remove(&id.bits())
    }

    /// Set location for entity with specified ID.
    #[inline]
    pub fn set_location(&mut self, id: EntityId, loc: Location) {
        self.map.insert(id.bits(), loc);
    }

    /// Returns location for entity with specified ID.
    #[inline]
    pub fn get_location(&self, id: EntityId) -> Option<Location> {
        match self.map.get(&id.bits()) {
            None => {
                let reserved = self.reserve_counter.load(Ordering::Acquire);
                let Some(idx) = self.id_allocator.reserved(id.non_zero()) else {
                    return None;
                };
                if idx >= reserved {
                    return None;
                }
                Some(Location::reserved(idx as u32))
            }
            Some(loc) => Some(*loc),
        }
    }

    /// Returns location for entity with specified ID.
    #[inline]
    pub fn is_alive(&self, id: EntityId) -> bool {
        if self.map.contains_key(&id.bits()) {
            return true;
        }
        let reserved = self.reserve_counter.load(Ordering::Acquire);
        let Some(idx) = self.id_allocator.reserved(id.non_zero()) else {
            return false;
        };
        idx < reserved
    }

    /// Reserves capacity for at least `additional` more
    /// entities to be inserted.
    pub fn reserve(&mut self, additional: u32) {
        self.map.reserve(additional as usize);
    }
}
