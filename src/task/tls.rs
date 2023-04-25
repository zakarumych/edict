use core::{cell::Cell, ptr::NonNull};

use crate::world::World;

std::thread_local! {
    static WORLD_TLS: Cell<Option<NonNull<World>>> = Cell::new(None);
}

pub(super) struct WorldTLS {
    this: NonNull<World>,
    prev: Option<NonNull<World>>,
}

impl WorldTLS {
    pub fn new(world: &mut World) -> Self {
        let this = NonNull::from(world);
        let prev = WORLD_TLS.with(|tls| tls.replace(Some(this)));
        WorldTLS { this, prev }
    }

    pub unsafe fn get<'a>() -> &'a mut World {
        WORLD_TLS.with(|tls| unsafe { tls.get().unwrap().as_mut() })
    }
}

impl Drop for WorldTLS {
    fn drop(&mut self) {
        WORLD_TLS.with(|tls| {
            debug_assert_eq!(tls.get(), Some(self.this));
            tls.set(self.prev)
        });
    }
}
