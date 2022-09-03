use core::{
    num::NonZeroU32,
    sync::atomic::{AtomicI32, Ordering},
};

use alloc::{fmt, vec::Vec};

use crate::world::NoSuchEntity;

use super::EntityId;

/// Stores entity information in the World
struct EntityData {
    /// Entity generation.
    gen: u32,

    /// Archetype index.
    archetype: u32,

    /// Index within archetype.
    idx: u32,
}

impl EntityData {
    pub fn new(archetype: u32, idx: u32, id: u32, gen: NonZeroU32) -> (Self, EntityId) {
        let id = EntityId::new(id, gen);
        let data = EntityData {
            archetype,
            gen: gen.get(),
            idx,
        };
        (data, id)
    }

    pub fn entity(&self, id: u32) -> Option<EntityId> {
        Some(EntityId::new(id, NonZeroU32::new(self.gen)?))
    }
}

impl fmt::Debug for EntityData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityData")
            .field("gen", &self.gen)
            .field("archetype", &self.archetype)
            .field("idx", &self.idx)
            .finish()
    }
}

pub(crate) struct Entities {
    array: Vec<EntityData>,
    free_entity_ids: Vec<u32>,
    reserve_counter: AtomicI32,
}

impl fmt::Debug for Entities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entities")
            .field("entities", &self.array)
            .finish_non_exhaustive()
    }
}

impl Entities {
    pub fn new() -> Self {
        Entities {
            array: Vec::new(),
            free_entity_ids: Vec::new(),
            reserve_counter: AtomicI32::new(0),
        }
    }

    pub fn spawn(&mut self) -> EntityId {
        match self.free_entity_ids.pop() {
            None => {
                let (data, id) = EntityData::new(0, u32::MAX, self.array.len() as u32, first_gen());
                self.array.push(data);
                id
            }
            Some(id) => {
                let data = &self.array[id as usize];
                unsafe {
                    // # Safety
                    // Exhausted slots are not placed into free list
                    data.entity(id).unwrap_unchecked()
                }
            }
        }
    }

    pub fn reserve(&self) -> EntityId {
        let counter = self.reserve_counter.fetch_sub(1, Ordering::Release);

        if counter > 0 {
            let index = counter as usize - 1;
            let id = self.free_entity_ids[index];

            let data = &self.array[id as usize];
            unsafe {
                // # Safety
                // Exhausted slots are not placed into free list
                data.entity(id).unwrap_unchecked()
            }
        } else {
            let id = self.array.len() as u32 + (-counter) as u32;
            EntityId::new(id, first_gen())
        }
    }

    pub fn spawn_reserved(&mut self, mut f: impl FnMut(EntityId) -> u32) {
        let reserve_counter = self.reserve_counter.get_mut();

        // A tail of free_entity_ids was consumed by `reserve` method.
        for id in self
            .free_entity_ids
            .drain(0.max(*reserve_counter) as usize..)
        {
            let entity = unsafe {
                // # Safety
                // Exhausted slots are not placed into free list
                self.array[id as usize].entity(id).unwrap_unchecked()
            };
            let idx = f(entity);
            self.array[id as usize].idx = idx;
        }

        if *reserve_counter < 0 {
            // Spawn reserved entities.
            let reserved_count = (-*reserve_counter) as usize;
            self.array.reserve(reserved_count);

            for _ in 0..reserved_count {
                let (data, id) = EntityData::new(0, u32::MAX, self.array.len() as u32, first_gen());
                self.array.push(data);
                let idx = f(id);
                let last = self.array.last_mut().unwrap();
                last.idx = idx;
            }
        }

        *reserve_counter = 0;
    }

    pub fn despawn(&mut self, id: EntityId) -> Result<(u32, u32), NoSuchEntity> {
        if self.array.len() as u32 <= id.id() {
            return Err(NoSuchEntity);
        }
        let data = &mut self.array[id.id() as usize];
        if id.gen().get() != data.gen {
            return Err(NoSuchEntity);
        }

        if data.gen != u32::MAX {
            data.gen += 1;
            data.archetype = 0;
            data.idx = u32::MAX;
            self.free_entity_ids.push(id.id());
            *self.reserve_counter.get_mut() = self.free_entity_ids.len() as i32;
        } else {
            data.gen = 0;
        }
        Ok((data.archetype, data.idx))
    }

    pub fn set_location(&mut self, id: u32, archetype: u32, idx: u32) {
        let data = &mut self.array[id as usize];
        data.archetype = archetype;
        data.idx = idx;
    }

    pub fn find_entity(&self, id: u32) -> Option<EntityId> {
        if self.array.len() as u32 <= id {
            let reserved = (-self.reserve_counter.load(Ordering::Acquire)) as u32;
            if self.array.len() as u32 + reserved > id {
                Some(EntityId::new(id, first_gen()))
            } else {
                None
            }
        } else {
            let data = &self.array[id as usize];
            Some(EntityId::new(id, NonZeroU32::new(data.gen)?))
        }
    }

    pub fn get_location(&self, id: EntityId) -> Option<(u32, u32)> {
        if self.array.len() as u32 <= id.id() {
            if id.gen() != first_gen() {
                return None;
            }
            let reserved = (-self.reserve_counter.load(Ordering::Acquire)) as u32;
            if self.array.len() as u32 + reserved > id.id() {
                return Some((0, u32::MAX));
            }
            return None;
        }
        if id.gen().get() != self.array[id.id() as usize].gen {
            return None;
        }
        let data = &self.array[id.id() as usize];
        Some((data.archetype, data.idx))
    }
}

pub(super) fn invalid_gen() -> NonZeroU32 {
    NonZeroU32::new(1).unwrap()
}

pub(super) fn first_gen() -> NonZeroU32 {
    NonZeroU32::new(2).unwrap()
}
