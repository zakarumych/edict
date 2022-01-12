pub(crate) use self::entities::Entities;
pub use self::{id::EntityId, typed::Entity, weak::WeakEntity};

mod entities;
mod id;
mod queue;
mod strong;
mod typed;
mod weak;

impl PartialEq<WeakEntity> for EntityId {
    #[inline]
    fn eq(&self, other: &WeakEntity) -> bool {
        self.id == other.id
    }
}

impl<T> PartialEq<Entity<T>> for EntityId {
    #[inline]
    fn eq(&self, other: &Entity<T>) -> bool {
        self.id == other.id
    }
}

impl PartialEq<EntityId> for WeakEntity {
    #[inline]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == other.id
    }
}

impl<T> PartialEq<Entity<T>> for WeakEntity {
    #[inline]
    fn eq(&self, other: &Entity<T>) -> bool {
        self.gen == other.gen && self.id == other.id
    }
}

impl<T> PartialEq<EntityId> for Entity<T> {
    #[inline]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == other.id
    }
}

impl<T> PartialEq<WeakEntity> for Entity<T> {
    #[inline]
    fn eq(&self, other: &WeakEntity) -> bool {
        self.gen == other.gen && self.id == other.id
    }
}
