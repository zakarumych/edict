use core::{any::TypeId, ptr::NonNull};

use crate::{
    archetype::Archetype, component::ComponentInfo, system::ActionBufferQueue, world::World, Access,
};

use super::{FnArg, FnArgState};

#[derive(Default)]
pub struct WorldReadState;

impl FnArg for &World {
    type State = WorldReadState;
}

unsafe impl FnArgState for WorldReadState {
    type Arg<'a> = &'a World;

    #[inline]
    fn new() -> Self {
        Self::default()
    }

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
        true
    }

    #[inline]
    fn borrows_components_at_runtime(&self) -> bool {
        true
    }

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn resource_type_access(&self, _ty: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> &'a World {
        // Safety: Declares read.
        unsafe { world.as_ref() }
    }
}

#[derive(Default)]
pub struct WorldWriteState;

impl FnArg for &mut World {
    type State = WorldWriteState;
}

unsafe impl FnArgState for WorldWriteState {
    type Arg<'a> = &'a mut World;

    #[inline]
    fn new() -> Self {
        Self::default()
    }

    #[inline]
    fn is_local(&self) -> bool {
        true
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    #[inline]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn resource_type_access(&self, _ty: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        mut world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> &'a mut World {
        // Safety: Declares write.
        unsafe { world.as_mut() }
    }
}
