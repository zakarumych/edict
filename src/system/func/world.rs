use core::{any::TypeId, ptr::NonNull};

use crate::{archetype::Archetype, query::Access, world::World};

use super::{FnArg, FnArgCache, FnArgGet};

#[derive(Default)]
pub struct WorldReadCache;

impl FnArg for &World {
    type Cache = WorldReadCache;
}

impl FnArgCache for WorldReadCache {
    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }
}

unsafe impl<'a> FnArgGet<'a> for WorldReadCache {
    type Arg = &'a World;

    #[inline]
    unsafe fn get_unchecked(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn crate::system::ActionQueue,
    ) -> &'a World {
        world.as_ref()
    }
}

#[derive(Default)]
pub struct WorldWriteCache;

impl FnArg for &mut World {
    type Cache = WorldWriteCache;
}

impl FnArgCache for WorldWriteCache {
    #[inline]
    fn is_local(&self) -> bool {
        true
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }
}

unsafe impl<'a> FnArgGet<'a> for WorldWriteCache {
    type Arg = &'a mut World;

    #[inline]
    unsafe fn get_unchecked(
        &'a mut self,
        mut world: NonNull<World>,
        _queue: &mut dyn crate::system::ActionQueue,
    ) -> &'a mut World {
        world.as_mut() // # Safety: Declares write access.
    }
}
