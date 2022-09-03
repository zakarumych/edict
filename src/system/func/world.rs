use core::any::TypeId;

use crate::{archetype::Archetype, query::Access, world::World};

use super::{FnArg, FnArgCache, FnArgGet};

#[derive(Default)]
pub struct WorldCache;

impl FnArg for &World {
    type Cache = WorldCache;
}

impl FnArgCache for WorldCache {
    fn is_local(&self) -> bool {
        false
    }

    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    fn access_component(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }
}

unsafe impl<'a> FnArgGet<'a> for WorldCache {
    type Arg = &'a World;

    unsafe fn get_unchecked(
        &'a mut self,
        world: &'a World,
        _queue: &mut dyn crate::system::ActionQueue,
    ) -> &'a World {
        world
    }
}
