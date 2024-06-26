//! Entity references.
//!
//! Alive, weak and raw ids.

pub use self::{
    allocator::{IdRange, IdRangeAllocator, OneRangeAllocator},
    entity::{AliveEntity, Entity, EntityBound, EntityId, EntityLoc, EntityRef},
    set::{EntitySet, Location},
};

mod allocator;
mod entity;
mod set;
