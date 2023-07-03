use core::{any::TypeId, ptr::NonNull};

use crate::{archetype::Archetype, query::Access, system::ActionQueue, world::World};

use super::{FnArg, FnArgState};

#[derive(Default)]
pub struct WorldReadState;

impl FnArg for &World {
    type State = WorldReadState;
}

unsafe impl FnArgState for WorldReadState {
    type Arg<'a> = &'a World;

    #[inline(always)]
    fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        false
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline(always)]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    #[inline(always)]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline(always)]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
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

    #[inline(always)]
    fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        true
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline(always)]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    #[inline(always)]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline(always)]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        mut world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> &'a mut World {
        // Safety: Declares write.
        unsafe { world.as_mut() }
    }
}
