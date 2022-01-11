use core::{fmt, marker::PhantomData, mem::ManuallyDrop, num::NonZeroU32, ops::Deref};

use crate::{
    bundle::Bundle,
    handle::{Handle, Queue},
    world::World,
};

/// Owning reference to an entity.
/// This value can be used to access an entity and keeps the entity alive.
/// Guarantees no strong references to the entity exists.
/// Dropping this value will despawn the entity immediately.
///
/// This type is generic over list of pinned components.
/// Pinned components cannot be removed and thus they can be accessed with guarantee.
pub struct OwnedEntity<T = ()> {
    weak: WeakEntity,
    handle: ManuallyDrop<Handle>,
    marker: PhantomData<fn() -> T>,
}

impl<T> Drop for OwnedEntity<T> {
    fn drop(&mut self) {
        let handle = unsafe { ManuallyDrop::take(&mut self.handle) };
        handle.drop(self.weak.id);
    }
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
    };

    (impl $($a:ident)+) => {
    };
}

for_tuple!();
