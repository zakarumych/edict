use core::{any::TypeId, cell::Cell, ptr::NonNull};

use parking_lot::Mutex;

use crate::{entity::EntityId, world::World};

std::thread_local! {
    static WORLD_TLS: Cell<Option<NonNull<World>>> = Cell::new(None);
    static QUEUE_TLS: Mutex<Vec<(EntityId, TypeId)>> = Mutex::new(Vec::new());
}

pub(super) struct WorldTLS {
    _private: (),
}

impl WorldTLS {
    pub fn new(world: &mut World) -> Self {
        WORLD_TLS.with(|tls| tls.set(Some(NonNull::from(world))));
        Self { _private: () }
    }

    pub unsafe fn get<'a>() -> &'a mut World {
        WORLD_TLS.with(|tls| unsafe { tls.get().unwrap().as_mut() })
    }
}

impl Drop for WorldTLS {
    fn drop(&mut self) {
        WORLD_TLS.with(|tls| tls.take());
    }
}

pub(super) fn enqueue<T: 'static>(id: EntityId) {
    QUEUE_TLS.with(|tls| tls.lock().push((id, TypeId::of::<T>())));
}

pub(super) fn deque(queue: &mut Vec<(EntityId, TypeId)>) {
    QUEUE_TLS.with(|tls| queue.append(&mut *tls.lock()))
}
