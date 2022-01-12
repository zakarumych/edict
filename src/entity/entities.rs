use core::{
    marker::PhantomData,
    num::NonZeroU32,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{boxed::Box, fmt, vec::Vec};

use super::{queue::DropQueue, strong::StrongEntity, Entity, WeakEntity};

/// Stores entity information in the World
struct EntityData {
    /// Entity generation.
    gen: u32,

    /// Archetype index.
    archetype: u32,

    /// Index within archetype.
    idx: u32,

    shared: NonNull<EntityDataShared>,
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

pub(super) struct EntityDataShared {
    pub refs: AtomicUsize,
    pub queue: DropQueue,
}

pub(crate) struct Entities {
    array: Vec<EntityData>,
    free_entity_ids: Vec<u32>,
    queue: DropQueue,
}

impl fmt::Debug for Entities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entities")
            .field("entities", &self.array)
            .finish_non_exhaustive()
    }
}

impl Entities {
    pub fn new(inline_cap: usize) -> Self {
        let queue = DropQueue::new(inline_cap);

        Entities {
            array: Vec::new(),
            free_entity_ids: Vec::new(),
            queue,
        }
    }

    pub fn spawn(&mut self) -> Entity {
        match self.free_entity_ids.pop() {
            None => {
                let id = self.array.len() as u32;
                let gen = first_gen();

                let ptr = Box::new(EntityDataShared {
                    refs: AtomicUsize::new(1),
                    queue: self.queue.clone(),
                });

                let shared = NonNull::from(Box::leak(ptr));

                self.array.push(EntityData {
                    gen: gen.get(),
                    idx: 0,
                    archetype: 0,
                    shared,
                });

                Entity {
                    strong: StrongEntity {
                        weak: WeakEntity::new(id, gen),
                        shared,
                    },
                    marker: PhantomData,
                }
            }
            Some(id) => {
                let data = &self.array[id as usize];
                let gen = NonZeroU32::new(data.gen).expect("Exhausted slot");

                unsafe { &*data.shared.as_ptr() }
                    .refs
                    .store(1, Ordering::Relaxed);

                Entity {
                    strong: StrongEntity {
                        weak: WeakEntity::new(id, gen),
                        shared: data.shared,
                    },
                    marker: PhantomData,
                }
            }
        }
    }

    pub fn set_location(&mut self, id: u32, archetype: u32, idx: u32) {
        let data = &mut self.array[id as usize];
        data.archetype = archetype;
        data.idx = idx;
    }

    pub fn get(&self, weak: &WeakEntity) -> Option<(u32, u32)> {
        if self.array.len() as u32 <= weak.id {
            return None;
        }
        if weak.gen.get() != self.array[weak.id as usize].gen {
            return None;
        }
        let data = &self.array[weak.id as usize];
        Some((data.archetype, data.idx))
    }

    pub fn drop_queue(&self) -> DropQueue {
        self.queue.clone()
    }

    pub fn dropped(&mut self, id: u32) -> (u32, u32) {
        let data = &mut self.array[id as usize];
        if data.gen != u32::MAX {
            data.gen += 1;
            self.free_entity_ids.push(id);
            (data.archetype, data.idx)
        } else {
            data.gen = 0;
            (data.archetype, data.idx)
        }
    }

    pub fn is_owner_of<T>(&self, entity: &Entity<T>) -> bool {
        unsafe { &*entity.strong.shared.as_ptr() }.queue == self.queue
    }
}

pub(super) fn invalid_gen() -> NonZeroU32 {
    NonZeroU32::new(1).unwrap()
}

pub(super) fn first_gen() -> NonZeroU32 {
    NonZeroU32::new(2).unwrap()
}
