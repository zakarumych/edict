//! Entity references.
//!
//! Strong, weak and raw ids.

use core::marker::PhantomData;

use crate::world::World;

pub use self::allocator::{IdRange, IdRangeAllocator, OneRangeAllocator};
pub(crate) use self::entities::EntitySet;
use self::entities::Location;
pub use self::id::EntityId;

mod allocator;
mod entities;
mod id;

/// Entity location is known and is valid
/// for the lifetime of the borrow.
#[derive(Clone, Copy)]
pub struct KnownLocation<'a> {
    loc: Location,

    /// World borrow is bound to this lifetime.
    /// Meaning that entity's location cannot be changed
    /// while this borrow is alive.
    bound: PhantomData<&'a World>,
}

/// Entity location is unknown.
/// Perform entity lookup to get the location.
#[derive(Clone, Copy)]
pub struct UnknownLocation;

pub trait EntityLocation {
    fn lookup<'a>(&'a self, id: EntityId, entities: &'a EntitySet) -> Option<KnownLocation<'a>>;
}

impl EntityLocation for KnownLocation<'_> {
    #[inline(always)]
    fn lookup<'a>(&'a self, _id: EntityId, entities: &'a EntitySet) -> Option<KnownLocation<'a>> {
        Some(*self)
    }
}

impl EntityLocation for UnknownLocation {
    #[inline(always)]
    fn lookup<'a>(&'a self, id: EntityId, entities: &'a EntitySet) -> Option<KnownLocation<'a>> {
        let loc = entities.get_location(id)?;
        Some(KnownLocation {
            loc,
            bound: PhantomData,
        })
    }
}

/// Entity reference.
/// The kind of the reference is determined by generic parameters.
pub struct Entity<L = UnknownLocation> {
    id: EntityId,
    location: L,
}
