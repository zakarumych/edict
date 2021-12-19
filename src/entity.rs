use std::marker::PhantomData;

/// Entity weak reference.
/// This value can be used to access an entity, but it does not keep the entity alive.
#[derive(Clone, Copy, Debug)]
pub struct WeakEntity {}

/// Entity strong reference.
/// This value can be used to access an entity and keeps the entity alive.
pub struct StrongEntity<T> {
    marker: PhantomData<fn() -> T>,
}

/// Entity owning reference.
/// This value can be used to access an entity and keeps the entity alive.
/// Guarantees no strong references to the entity exists.
/// Dropping this value will despawn the entity immediately.
pub struct Entity<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Entity<T> {
    pub fn weak(&self) -> WeakEntity {
        WeakEntity {}
    }
}

#[test]
fn test_tokens_deref() {
    use crate::{proof::Skip, world::World};

    struct Foo;
    struct Bar;
    struct Baz;

    fn foo(world: &World, mut e: Entity<(Foo, Bar, Baz)>) {
        let (&mut ref mut foo, &ref bar, Skip, opt) = world.query_one_mut(&mut e);
        opt.unwrap_or(&42);

        let (&ref foo, Skip, Skip) = world.query_one(&e);
    }
}
