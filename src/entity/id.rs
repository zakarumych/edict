use core::{cmp::Ordering, fmt, num::NonZeroU64};

/// Unique identifier of an entity.
/// The identifier is unique within the world and
/// can be made unique across multiple worlds by
/// specifying custom id allocator.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct EntityId {
    value: NonZeroU64,
}

impl PartialOrd for EntityId {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.order().cmp(&other.order()))
    }
}

impl Ord for EntityId {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.order().cmp(&other.order())
    }
}

impl EntityId {
    #[inline]
    pub(crate) fn new(value: NonZeroU64) -> Self {
        EntityId { value }
    }

    #[inline]
    pub(crate) fn value(&self) -> NonZeroU64 {
        self.value
    }

    /// Returns expired entity id.
    ///
    /// This function exists primarily to make dummy EntityId values.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, entity::EntityId};
    /// # let mut world = World::new();
    /// let id = EntityId::dangling();
    /// assert_eq!(world.is_alive(id), false);
    /// ```
    #[inline]
    pub fn dangling() -> Self {
        EntityId {
            // Safety: 1 is not 0.
            value: NonZeroU64::new(1).unwrap(),
        }
    }

    /// Gets 64-bit integer that can be converted back to equal `EntityId`.
    #[inline]
    pub fn bits(&self) -> u64 {
        self.value.get()
    }

    /// Converts 64-bit integer to `EntityId`.
    /// Returns `None` for integer less than or equal to `u32::MAX`.
    #[inline]
    pub fn from_bits(bits: u64) -> Option<Self> {
        let value = NonZeroU64::new(bits)?;
        Some(EntityId { value })
    }

    // /// Returns generation part of the entity id.
    // #[inline]
    // pub fn gen(&self) -> NonZeroU32 {
    //     unsafe { NonZeroU32::new_unchecked((self.value.get() >> 32) as u32) }
    // }

    // /// Returns id part of the entity id.
    // #[inline]
    // pub fn id(&self) -> u32 {
    //     self.value.get() as u32
    // }

    #[inline]
    fn order(&self) -> u64 {
        self.value.get()
    }
}

impl fmt::Debug for EntityId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityId")
            .field("value", &self.value.get())
            .finish()
    }
}

impl fmt::Display for EntityId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{:0x}}}", self.value)
    }
}
