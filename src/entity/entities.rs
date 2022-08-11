use core::num::NonZeroU32;

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
        }
    }

    pub fn spawn(&mut self) -> EntityId {
        match self.free_entity_ids.pop() {
            None => {
                let id = self.array.len() as u32;
                let gen = first_gen();

                self.array.push(EntityData {
                    gen: gen.get(),
                    idx: 0,
                    archetype: 0,
                });

                EntityId::new(id, gen)
            }
            Some(id) => {
                let data = &self.array[id as usize];
                let gen = NonZeroU32::new(data.gen).expect("Exhausted slot");

                EntityId::new(id, gen)
            }
        }
    }

    pub fn despawn(&mut self, id: EntityId) -> Result<(u32, u32), NoSuchEntity> {
        if self.array.len() as u32 <= id.idx() {
            return Err(NoSuchEntity);
        }
        let data = &mut self.array[id.idx() as usize];
        if id.gen().get() != data.gen {
            return Err(NoSuchEntity);
        }

        if data.gen != u32::MAX {
            data.gen += 1;
            self.free_entity_ids.push(id.idx());
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
            return None;
        }
        let data = &self.array[id as usize];
        Some(EntityId::new(id, NonZeroU32::new(data.gen)?))
    }

    pub fn get(&self, id: EntityId) -> Option<(u32, u32)> {
        if self.array.len() as u32 <= id.idx() {
            return None;
        }
        if id.gen().get() != self.array[id.idx() as usize].gen {
            return None;
        }
        let data = &self.array[id.idx() as usize];
        Some((data.archetype, data.idx))
    }
}

pub(super) fn invalid_gen() -> NonZeroU32 {
    NonZeroU32::new(1).unwrap()
}

pub(super) fn first_gen() -> NonZeroU32 {
    NonZeroU32::new(2).unwrap()
}
