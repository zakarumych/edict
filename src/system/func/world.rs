use core::marker::PhantomData;

use crate::{archetype::Archetype, world::World};

use super::{FnArg, FnArgCache, FnArgGet};

pub struct WorldCache;

impl FnArg for &mut World {
    type Cache = WorldCache;
}
