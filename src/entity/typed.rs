use core::{fmt, marker::PhantomData, num::NonZeroU32, ops::Deref};

use crate::bundle::Bundle;

use super::{
    strong::{DropQueue, StrongEntity},
    weak::WeakEntity,
};

/// Strong reference to an entity.
/// This value can be used to access an entity and keeps the entity alive.
/// On access to a component, if entity doesn't have accessed component,
/// an error is returned.
///
/// This type is generic over list of pinned components.
/// Pinned components cannot be removed and thus they can be accessed with guarantee.
#[derive(Clone, PartialEq, Eq)]
pub struct Entity<T = ()> {
    strong: StrongEntity,
    marker: PhantomData<fn() -> T>,
}

impl Entity {
    pub(crate) fn new(id: u32, gen: NonZeroU32, queue: &DropQueue) -> Self {
        Entity::from_weak(WeakEntity::new(id, gen), queue)
    }

    pub(crate) fn from_weak(weak: WeakEntity, queue: &DropQueue) -> Self {
        Entity {
            strong: StrongEntity::new(weak, queue),
            marker: PhantomData,
        }
    }

    pub(crate) fn with_bundle<B>(self) -> Entity<B>
    where
        B: Bundle,
    {
        Entity {
            strong: self.strong,
            marker: PhantomData,
        }
    }
}

impl<T> fmt::Debug for Entity<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entity")
            .field("gen", &self.gen.get())
            .field("id", &self.id)
            .finish()
    }
}

impl<T> fmt::Display for Entity<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.strong.weak, f)
    }
}

impl<T> Deref for Entity<T> {
    type Target = WeakEntity;

    fn deref(&self) -> &WeakEntity {
        &self.strong.weak
    }
}
