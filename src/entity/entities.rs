use core::num::NonZeroU32;

#[cfg(feature = "rc")]
use core::{
    marker::PhantomData,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{fmt, vec::Vec};

#[cfg(feature = "rc")]
use alloc::boxed::Box;

#[cfg(feature = "rc")]
use crate::world::OwnershipError;

use crate::world::NoSuchEntity;

#[cfg(feature = "rc")]
use super::{queue::DropQueue, strong::StrongInner, typed::Entity};

use super::EntityId;

/// Stores entity information in the World
struct EntityData {
    /// Entity generation.
    gen: u32,

    /// Archetype index.
    archetype: u32,

    /// Index within archetype.
    idx: u32,

    #[cfg(feature = "rc")]
    shared: Option<NonNull<EntityDataShared>>,
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

#[cfg(feature = "rc")]
pub(super) struct EntityDataShared {
    pub refs: AtomicUsize,
    pub queue: DropQueue,
}

#[cfg(feature = "rc")]
impl EntityDataShared {
    fn alloc(queue: DropQueue) -> NonNull<Self> {
        let ptr = Box::new(EntityDataShared {
            refs: AtomicUsize::new(1),
            queue,
        });

        NonNull::from(Box::leak(ptr))
    }
}

pub(crate) struct Entities {
    array: Vec<EntityData>,
    free_entity_ids: Vec<u32>,

    #[cfg(feature = "rc")]
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
    #[cfg(feature = "rc")]
    pub fn new(inline_cap: usize) -> Self {
        let queue = DropQueue::new(inline_cap);

        Entities {
            array: Vec::new(),
            free_entity_ids: Vec::new(),
            queue,
        }
    }

    #[cfg(not(feature = "rc"))]
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

                    #[cfg(feature = "rc")]
                    shared: None,
                });

                EntityId::new(id, gen)
            }
            Some(id) => {
                let data = &self.array[id as usize];
                let gen = NonZeroU32::new(data.gen).expect("Exhausted slot");

                #[cfg(feature = "rc")]
                if let Some(shared) = data.shared {
                    let refs = &unsafe { &*shared.as_ptr() }.refs;
                    debug_assert_eq!(refs.load(Ordering::Relaxed), 0);
                    refs.store(usize::MAX, Ordering::Relaxed);
                }

                EntityId::new(id, gen)
            }
        }
    }

    #[cfg(feature = "rc")]
    pub fn spawn_owning(&mut self) -> Entity {
        match self.free_entity_ids.pop() {
            None => {
                let id = self.array.len() as u32;
                let gen = first_gen();

                let shared = EntityDataShared::alloc(self.queue.clone());

                self.array.push(EntityData {
                    gen: gen.get(),
                    idx: 0,
                    archetype: 0,
                    shared: Some(shared),
                });

                Entity {
                    inner: StrongInner {
                        id: EntityId::new(id, gen),
                        shared,
                    },
                    marker: PhantomData,
                }
            }
            Some(id) => {
                let data = &mut self.array[id as usize];
                let gen = NonZeroU32::new(data.gen).expect("Exhausted slot");

                let shared = match data.shared {
                    None => {
                        let shared = EntityDataShared::alloc(self.queue.clone());
                        data.shared = Some(shared);
                        shared
                    }
                    Some(shared) => {
                        debug_assert_eq!(
                            unsafe { &*shared.as_ptr() }.refs.load(Ordering::Relaxed),
                            0
                        );
                        unsafe { &*shared.as_ptr() }
                            .refs
                            .store(1, Ordering::Relaxed);
                        shared
                    }
                };

                Entity {
                    inner: StrongInner {
                        id: EntityId::new(id, gen),
                        shared,
                    },
                    marker: PhantomData,
                }
            }
        }
    }

    #[cfg(feature = "rc")]
    pub fn take_ownership(&mut self, id: &EntityId) -> Result<Entity, OwnershipError> {
        if self.array.len() as u32 <= id.idx() {
            return Err(OwnershipError::NoSuchEntity);
        }
        let data = &mut self.array[id.idx() as usize];
        if id.gen().get() != data.gen {
            return Err(OwnershipError::NoSuchEntity);
        }

        let shared = match data.shared {
            None => {
                let shared = EntityDataShared::alloc(self.queue.clone());
                data.shared = Some(shared);
                shared
            }
            Some(shared) => {
                let refs = &unsafe { &*shared.as_ptr() }.refs;
                if refs.load(Ordering::Relaxed) != usize::MAX {
                    return Err(OwnershipError::NotOwned);
                }
                refs.store(1, Ordering::Relaxed);
                shared
            }
        };

        Ok(Entity {
            inner: StrongInner { id: *id, shared },
            marker: PhantomData,
        })
    }

    #[cfg(feature = "rc")]
    pub fn give_ownership<T>(&mut self, entity: Entity<T>) {
        debug_assert!(self.is_owner_of(&entity));

        let data = &mut self.array[entity.idx() as usize];

        match data.shared {
            None => unsafe { unreachable_unchecked() },
            Some(shared) => {
                let refs = &unsafe { &*shared.as_ptr() }.refs;
                if refs.load(Ordering::Relaxed) != 1 {
                    unsafe { unreachable_unchecked() }
                }
                refs.store(usize::MAX, Ordering::Relaxed);
            }
        };
    }

    pub fn despawn(&mut self, id: &EntityId) -> Result<(u32, u32), DespawnError> {
        if self.array.len() as u32 <= id.idx() {
            return Err(NoSuchEntity.into());
        }
        let data = &mut self.array[id.idx() as usize];
        if id.gen().get() != data.gen {
            return Err(NoSuchEntity.into());
        }

        #[cfg(feature = "rc")]
        if let Some(shared) = data.shared {
            if unsafe { &*shared.as_ptr() }.refs.load(Ordering::Relaxed) != usize::MAX {
                return Err(DespawnError::NotOwned);
            }
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

    pub fn get_entity(&self, id: u32) -> Option<EntityId> {
        if self.array.len() as u32 <= id {
            return None;
        }
        let data = &self.array[id as usize];
        Some(EntityId::new(id, NonZeroU32::new(data.gen)?))
    }

    pub fn get(&self, id: &EntityId) -> Option<(u32, u32)> {
        if self.array.len() as u32 <= id.idx() {
            return None;
        }
        if id.gen().get() != self.array[id.idx() as usize].gen {
            return None;
        }
        let data = &self.array[id.idx() as usize];
        Some((data.archetype, data.idx))
    }

    #[cfg(feature = "rc")]
    pub fn drop_queue(&self) -> DropQueue {
        self.queue.clone()
    }

    #[cfg(feature = "rc")]
    pub fn dropped(&mut self, idx: u32) -> (u32, u32) {
        let data = &mut self.array[idx as usize];
        if data.gen != u32::MAX {
            data.gen += 1;
            self.free_entity_ids.push(idx);
        } else {
            data.gen = 0;
        }
        (data.archetype, data.idx)
    }

    #[cfg(feature = "rc")]
    pub fn is_owner_of<T>(&self, entity: &Entity<T>) -> bool {
        unsafe { &*entity.inner.shared.as_ptr() }.queue == self.queue
    }
}

#[cfg(feature = "rc")]
type DespawnError = OwnershipError;

#[cfg(not(feature = "rc"))]
type DespawnError = NoSuchEntity;

pub(super) fn invalid_gen() -> NonZeroU32 {
    NonZeroU32::new(1).unwrap()
}

pub(super) fn first_gen() -> NonZeroU32 {
    NonZeroU32::new(2).unwrap()
}

#[cfg(feature = "rc")]
#[cfg(debug_assertions)]
unsafe fn unreachable_unchecked() {
    unreachable!();
}

#[cfg(feature = "rc")]
#[inline]
#[cfg(not(debug_assertions))]
unsafe fn unreachable_unchecked() {
    core::hint::unreachable_unchecked()
}
