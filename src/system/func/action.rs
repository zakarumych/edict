use core::any::TypeId;

use crate::{
    action::ActionEncoder, archetype::Archetype, query::Access, system::ActionQueue, world::World,
};

use super::{FnArg, FnArgCache, FnArgGet};

impl FnArg for &mut ActionEncoder {
    type Cache = ActionEncoderCache;
}

#[derive(Default)]
pub struct ActionEncoderCache {
    encoder: Option<ActionEncoder>,
}

impl FnArgCache for ActionEncoderCache {
    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
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
    type Arg = &'a mut ActionEncoder;

    #[inline]
    unsafe fn get_unchecked(
        &'a mut self,
        _world: &'a World,
        queue: &mut dyn ActionQueue,
    ) -> &'a mut ActionEncoder {
        self.encoder
            .get_or_insert_with(|| queue.get_action_encoder())
    }

    #[inline]
    unsafe fn flush_unchecked(&'a mut self, _world: &'a World, queue: &mut dyn ActionQueue) {
        if let Some(encoder) = self.encoder.take() {
            queue.flush_action_encoder(encoder);
        }
    }
}
