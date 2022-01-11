use core::{fmt, num::NonZeroU32};

/// Weak reference to an entity.
/// This value can be used to access an entity, but it does not keep the entity alive.
/// On access to a component, if entity is expired (no strong refs left) or doesn't have accessed component,
/// corresponding error is returned.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WeakEntity {
    pub(crate) gen: NonZeroU32,
    pub(crate) id: u32,
}

impl WeakEntity {
    pub(crate) fn new(id: u32, gen: NonZeroU32) -> Self {
        WeakEntity { gen, id }
    }
}

impl fmt::Debug for WeakEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WeakEntity")
            .field("gen", &self.gen.get())
            .field("id", &self.id)
            .finish()
    }
}

impl fmt::Display for WeakEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{:0x}#{:x}}}", self.gen.get(), self.id)
    }
}
