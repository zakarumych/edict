//! Entity references.
//!
//! Strong, weak and raw ids.

pub use self::allocator::{IdRange, IdRangeAllocator, OneRangeAllocator};
pub(crate) use self::entities::EntitySet;
pub use self::id::EntityId;

mod allocator;
mod entities;
mod id;
