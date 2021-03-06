use core::{ptr::NonNull, sync::atomic::Ordering};

use super::{entities::EntityDataShared, id::EntityId};

#[derive(PartialEq, Eq)]
pub(super) struct StrongInner {
    pub id: EntityId,
    pub shared: NonNull<EntityDataShared>,
}

#[allow(dead_code)]
fn entity_data_shared_send_sync() {
    fn test<T: Send + Sync>() {}
    test::<EntityDataShared>();
}

/// # Safety
///
/// Referenced `EntityDataShared` is accessed only immutably.
/// `EntityDataShared` is `Send + Sync`.
unsafe impl Send for StrongInner {}

/// # Safety
///
/// Referenced `EntityDataShared` is accessed only immutably.
/// `EntityDataShared` is `Send + Sync`.
unsafe impl Sync for StrongInner {}

impl Drop for StrongInner {
    fn drop(&mut self) {
        let shared = unsafe { &*self.shared.as_ptr() };
        let old = shared.refs.fetch_sub(1, Ordering::Release);
        if old == 1 {
            shared.queue.drop_entity(self.id.idx);
        }
    }
}

impl Clone for StrongInner {
    fn clone(&self) -> Self {
        unsafe {
            (*self.shared.as_ptr()).refs.fetch_add(1, Ordering::Relaxed);
        }
        StrongInner {
            id: self.id,
            shared: self.shared,
        }
    }
}
