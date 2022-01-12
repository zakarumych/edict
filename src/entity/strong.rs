use core::{ptr::NonNull, sync::atomic::Ordering};

use super::{entities::EntityDataShared, weak::WeakEntity};

#[derive(PartialEq, Eq)]
pub(super) struct StrongEntity {
    pub weak: WeakEntity,
    pub shared: NonNull<EntityDataShared>,
}

impl Drop for StrongEntity {
    fn drop(&mut self) {
        let shared = unsafe { &*self.shared.as_ptr() };
        let old = shared.refs.fetch_sub(1, Ordering::Release);
        if old == 1 {
            shared.queue.drop_entity(self.weak.id);
        }
    }
}

impl Clone for StrongEntity {
    fn clone(&self) -> Self {
        unsafe {
            (*self.shared.as_ptr()).refs.fetch_add(1, Ordering::Relaxed);
        }
        StrongEntity {
            weak: self.weak,
            shared: self.shared,
        }
    }
}
