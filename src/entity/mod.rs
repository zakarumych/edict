//! Entity references.
//!
//! Strong, weak and raw ids.

pub(crate) use self::entities::Entities;
pub use self::id::EntityId;

#[cfg(feature = "rc")]
pub use self::typed::{Entity, SharedEntity};

mod entities;
mod id;

#[cfg(feature = "rc")]
mod queue;

#[cfg(feature = "rc")]
mod strong;

#[cfg(feature = "rc")]
mod typed;

#[cfg(feature = "rc")]
impl<T> PartialEq<SharedEntity<T>> for EntityId {
    #[inline]
    fn eq(&self, other: &SharedEntity<T>) -> bool {
        self.gen == other.gen && self.idx == other.idx
    }
}

#[cfg(feature = "rc")]
impl<T> PartialEq<EntityId> for SharedEntity<T> {
    #[inline]
    fn eq(&self, other: &EntityId) -> bool {
        self.gen == other.gen && self.idx == other.idx
    }
}
