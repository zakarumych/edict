use core::{fmt, num::NonZeroU32};

use super::entities::invalid_gen;

/// Weak reference to an entity.
/// This value can be used to access an entity, but it does not keep the entity alive.
/// On access to a component, if entity is expired (no strong refs left) or doesn't have accessed component,
/// corresponding error is returned.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    pub(crate) gen: NonZeroU32,
    pub(crate) idx: u32,
}

impl EntityId {
    pub(crate) fn new(id: u32, gen: NonZeroU32) -> Self {
        EntityId { gen, idx: id }
    }

    /// Returns expired weak entity.
    ///
    /// This function exists primarily to make dummy EntityId values.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::prelude::{World, EntityId};
    /// # let mut world = World::new();
    /// let weak = EntityId::dangling();
    /// assert_eq!(world.is_alive(&weak), false);
    /// ```
    pub fn dangling() -> Self {
        EntityId::new(0, invalid_gen())
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityId")
            .field("gen", &self.gen.get())
            .field("id", &self.idx)
            .finish()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{:0x}#{:x}}}", self.gen.get(), self.idx)
    }
}
