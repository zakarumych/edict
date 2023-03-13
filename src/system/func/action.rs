use core::{any::TypeId, ptr::NonNull};

use crate::{
    action::{ActionBuffer, ActionEncoder},
    archetype::Archetype,
    query::Access,
    system::ActionQueue,
    world::World,
};

use super::{FnArg, FnArgCache, FnArgGet};

impl FnArg for ActionEncoder<'_> {
    type Cache = ActionEncoderCache;
}

/// [`FnArgCache`] for `ActionEncoder` argument.
#[derive(Default)]
pub struct ActionEncoderCache {
    buffer: Option<ActionBuffer>,
}

impl FnArgCache for ActionEncoderCache {
    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        None
    }
}

unsafe impl<'a> FnArgGet<'a> for ActionEncoderCache {
    type Arg = ActionEncoder<'a>;

    #[inline]
    unsafe fn get_unchecked(
        &'a mut self,
        world: NonNull<World>,
        queue: &mut dyn ActionQueue,
    ) -> ActionEncoder<'a> {
        let buffer = self.buffer.get_or_insert_with(|| queue.get());
        ActionEncoder::new(buffer, world.as_ref().entity_set())
    }

    #[inline]
    unsafe fn flush_unchecked(&'a mut self, _world: NonNull<World>, queue: &mut dyn ActionQueue) {
        if let Some(buffer) = self.buffer.take() {
            queue.flush(buffer);
        }
    }
}
