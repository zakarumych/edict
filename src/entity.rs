use core::{fmt, marker::PhantomData, num::NonZeroU32, ops::Deref};

use alloc::sync::Arc;

use crate::{bundle::Bundle, component::Component, world::World};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct EntityId {
    pub id: u32,
}

impl PartialEq<WeakEntity> for EntityId {
    #[inline]
    fn eq(&self, other: &WeakEntity) -> bool {
        self.id == other.id
    }
}

impl<T> PartialEq<Entity<T>> for EntityId {
    #[inline]
    fn eq(&self, other: &Entity<T>) -> bool {
        self.id == other.weak.id
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityId").field("id", &self.id).finish()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{:X}}}", self.id)
    }
}

/// Weak reference to an entity.
/// This value can be used to access an entity, but it does not keep the entity alive.
/// On access to a component, if entity is expired (no strong refs left) or doesn't have accessed component,
/// corresponding error is returned.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WeakEntity {
    pub(crate) gen: NonZeroU32,
    pub(crate) id: u32,
}

impl PartialEq<EntityId> for WeakEntity {
    #[inline]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == other.id
    }
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

/// Strong reference to an entity.
/// This value can be used to access an entity and keeps the entity alive.
/// On access to a component, if entity doesn't have accessed component,
/// an error is returned.
///
/// This type is generic over list of pinned components.
/// Pinned components cannot be removed and thus they can be accessed with guarantee.
#[derive(Clone)]
pub struct Entity<T = ()> {
    weak: WeakEntity,
    refs: Arc<()>,
    marker: PhantomData<fn() -> T>,
}

impl<T> PartialEq for Entity<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.weak == other.weak
    }
}

impl<T> Eq for Entity<T> {}

impl<T> PartialEq<EntityId> for Entity<T> {
    #[inline]
    fn eq(&self, other: &EntityId) -> bool {
        self.weak.id == other.id
    }
}

impl Entity {
    pub(crate) fn new(id: u32, gen: NonZeroU32, refs: Arc<()>) -> Self {
        Entity {
            weak: WeakEntity { id, gen },
            refs,
            marker: PhantomData,
        }
    }

    pub(crate) fn with_bundle<B>(self) -> Entity<B>
    where
        B: Bundle,
    {
        Entity {
            weak: self.weak,
            refs: self.refs,
            marker: PhantomData,
        }
    }
}

impl<T> fmt::Debug for Entity<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entity")
            .field("gen", &self.weak.gen.get())
            .field("id", &self.weak.id)
            .finish()
    }
}

impl<T> fmt::Display for Entity<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.weak, f)
    }
}

impl<T> Deref for Entity<T> {
    type Target = WeakEntity;

    fn deref(&self) -> &WeakEntity {
        &self.weak
    }
}

/// Owning reference to an entity.
/// This value can be used to access an entity and keeps the entity alive.
/// Guarantees no strong references to the entity exists.
/// Dropping this value will despawn the entity immediately.
///
/// This type is generic over list of pinned components.
/// Pinned components cannot be removed and thus they can be accessed with guarantee.
#[repr(transparent)]
pub struct OwnedEntity<T = ()> {
    weak: WeakEntity,
    marker: PhantomData<fn() -> T>,
}

impl<T> fmt::Debug for OwnedEntity<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedEntity")
            .field("gen", &self.weak.gen.get())
            .field("id", &self.weak.id)
            .finish()
    }
}

impl<T> fmt::Display for OwnedEntity<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.weak, f)
    }
}

impl<T> Deref for OwnedEntity<T> {
    type Target = WeakEntity;

    fn deref(&self) -> &WeakEntity {
        &self.weak
    }
}

impl<T> OwnedEntity<T> {
    pub fn share(self, world: &World) -> Entity<T> {
        todo!()
    }
}

#[test]
fn test_tokens_deref() {
    use crate::{proof::Skip, world::World};

    struct Foo;
    struct Bar;
    struct Baz;

    fn foo(world: &mut World, e: Entity<(Foo, Bar, Baz)>) {
        let (foo, bar, Skip, opt) = world.get_mut::<(&Foo, &mut Bar, Skip, Option<&u32>), _>(&e);
    }
}

macro_rules! for_tuple {
    () => {
        for_tuple!(for A B C D E F G);
    };

    (for) => {
        for_tuple!(impl);
    };

    (for $head:ident $($tail:ident)*) => {
        for_tuple!(for $($tail)*);
        for_tuple!(impl $head $($tail)*);
    };

    (impl) => {
        impl Entity<()> {
            pub(crate) fn add_one<T>(self) -> Entity<(T,)>
            where
                T: Component,
            {
                Entity {
                    weak: self.weak,
                    refs: self.refs,
                    marker: PhantomData,
                }
            }
        }
    };

    (impl $($a:ident)+) => {
    };
}

for_tuple!();
