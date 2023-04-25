use core::{cell::Cell, ptr::NonNull};

use crate::world::World;

std::thread_local! {
    static WORLD_TLS: Cell<Option<NonNull<World>>> = Cell::new(None);
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
