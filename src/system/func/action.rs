use core::{any::TypeId, ptr::NonNull};

use crate::{
    action::{ActionBuffer, ActionEncoder},
    archetype::Archetype,
    component::ComponentInfo,
    system::{Access, ActionBufferQueue},
    world::World,
};

use super::{FnArg, FnArgState};

impl FnArg for ActionEncoder<'_> {
    type State = ActionEncoderState;
}

/// [`FnArgState`] for `ActionEncoder` argument.
#[derive(Default)]
pub struct ActionEncoderState {
    buffer: Option<ActionBuffer>,
}

unsafe impl FnArgState for ActionEncoderState {
    type Arg<'a> = ActionEncoder<'a>;

    fn new() -> Self {
        ActionEncoderState { buffer: None }
    }

    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        None
    }

    #[inline]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Option<Access> {
        None
    }

    #[inline]
    fn resource_type_access(&self, _ty: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        queue: &mut dyn ActionBufferQueue,
    ) -> ActionEncoder<'a> {
        debug_assert!(self.buffer.is_none());
        let buffer = self.buffer.get_or_insert_with(|| queue.get());
        ActionEncoder::new(buffer, unsafe { world.as_ref() }.entities())
    }

    #[inline]
    unsafe fn flush_unchecked(
        &mut self,
        _world: NonNull<World>,
        queue: &mut dyn ActionBufferQueue,
    ) {
        if let Some(buffer) = self.buffer.take() {
            queue.flush(buffer);
        }
    }
}
