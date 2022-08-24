//! This example contains usage of the main features of Edict ECS.

use edict::world::World;

/// Just a type.
/// Being `'static` makes it a proper component type.
#[derive(Debug, PartialEq, Eq)]
struct Foo;

/// Another type.
#[derive(Debug, PartialEq, Eq)]
struct Bar;

/// Another type.
#[derive(Debug, PartialEq, Eq)]
struct Baz;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
struct Value<T>(T);

fn main() {
    // Create new World.
    let mut world_builder = World::builder();

    world_builder
        .register_external::<Foo>()
        .on_drop_fn(|_, _, _| {
            println!("Foo dropped");
        });

    world_builder.register_external::<Bar>();
    world_builder.register_external::<Baz>();
    world_builder.register_external::<Value<u8>>();
    world_builder.register_external::<Value<u16>>();
    world_builder.register_external::<Value<u32>>();

    let mut world = world_builder.build();

    // World doesn't not contain any entities yet.

    // Spawn new entity in the world and give it three components,
    // namely `Foo`, `Bar` and `Baz` components.
    // `World::spawn` takes a `Bundle` implementation.
    // Tuples of various size implement `Bundle` trait.
    // Using this method with tuple will cause all tuple elements
    // to be added as components to the entity.
    //
    // Take care to now try to add duplicate components in one bundle
    // as method will surely panic.
    let e = world.spawn_external((Foo, Bar, Baz));

    // Entity can be used to access components in the `World`.
    // Note that query returns `Result` because entity may be already despawned
    // or not have a component.
    assert!(matches!(world.query_one::<&Foo>(e), Ok(&Foo)));

    // To add another component to the entity call `World::insert`.
    world.insert_external(e, Value(0u32)).unwrap();
    assert!(matches!(world.query_one::<&Value<u32>>(e), Ok(&Value(0))));

    // If the component is already present in entity, the value is simply replaced.
    world.insert_external(e, Value(1u32)).unwrap();
    assert!(matches!(world.query_one::<&Value<u32>>(e), Ok(&Value(1))));

    // To add few components at once user should call `World::insert_bundle`.
    // This is much more efficient than adding components once by one.
    world
        .insert_external_bundle(e, (Value(1u8), Value(2u16)))
        .unwrap();

    // Spawned entities are despawned using [`World::despawn`] methods.
    world.despawn(e).unwrap();

    let _e = world.spawn_external((Foo, Bar));

    // Entities can be spawned in batches using iterators over bundles.
    // Each iterator element is treated as bundle of components
    // and spawned entities receive them.
    //
    // This is more efficient than spawning in loop,
    // especially if iterator size hints are more or less accurate
    // and not `(0, None)`
    //
    // `World::spawn_batch` returns an iterator with `Entity` for each entity created.
    let _entities: Vec<_> = world
        .spawn_batch_external((0..10u32).map(|i| (Value(i),)))
        .collect();

    // User may choose to not consume returned iterator, or consume it partially.
    // This would cause bundle iterator to not be consumed as well and entities will not be spawned.
    //
    // This allows using unbound iterators to produce entities and stop at any moment.
    //
    // Prefer using using bound iterators to allow edict to reserve space for all entities.
    let _entities: Vec<_> = world
        .spawn_batch_external((0u32..).map(|i| (Value(i),)))
        .take(10)
        .collect();
}
