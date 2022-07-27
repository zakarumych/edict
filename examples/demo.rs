//! This example contains usage of the main features of Edict ECS.
use edict::{prelude::*, relation::Relation};

/// Just a type.
/// Being `'static` makes it a proper component type.
#[derive(Debug, PartialEq, Eq)]
struct Foo;

impl Component for Foo {}

/// Another type.
#[derive(Debug, PartialEq, Eq)]
struct Bar;

impl Component for Bar {}
/// Another type.
#[derive(Debug, PartialEq, Eq)]
struct Baz;

impl Component for Baz {}

/// Child component type.
#[derive(Copy, Clone)]
struct ChildOf;

impl Relation for ChildOf {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
struct Value<T>(T);

impl<T> Component for Value<T> where T: Send + Sync + 'static {}

fn main() {
    // Create new World.
    let mut world_builder = World::builder();

    world_builder
        .register_component::<Foo>()
        .on_drop_fn(|_, _, _| {
            println!("Foo dropped");
        });

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
    let e = world.spawn((Foo, Bar, Baz));

    // Entity can be used to access components in the `World`.
    // Note that query returns `Result` because entity may be already despawned
    // or not have a component.
    assert!(matches!(world.query_one::<&Foo>(&e), Ok(&Foo)));

    // To add another component to the entity call `World::insert`.
    world.try_insert(&e, Value(0u32)).unwrap();
    assert!(matches!(world.query_one::<&Value<u32>>(&e), Ok(&Value(0))));

    // If the component is already present in entity, the value is simply replaced.
    world.try_insert(&e, Value(1u32)).unwrap();
    assert!(matches!(world.query_one::<&Value<u32>>(&e), Ok(&Value(1))));

    // To add few components at once user should call `World::insert_bundle`.
    // This is much more efficient than adding components once by one.
    world
        .try_insert_bundle(&e, (Value(1u8), Value(2u16)))
        .unwrap();

    // Spawned entities are despawned using [`World::despawn`] methods.
    world.despawn(&e).unwrap();

    let e = world.spawn((Foo, Bar));

    // Edict support taking ownership of the entity by user.
    let e_owning: Entity = world.take(&e).unwrap();

    // Now `e_owning` is owning reference to the entity (similar to Box).
    // Dropping owning reference would cause entity to be despawned.
    // At the same time `World::despawn` won't work on the entity.
    assert!(matches!(world.despawn(&e), Err(OwnershipError::NotOwned)));

    // Owning entity reference provides guarantee that entity is alive,
    // this property opens non-fallible API.
    // For example, inserting components cannot fail.
    world.insert(&e_owning, Value(42i32));

    // Owning entity can be used to access components through `World::get`
    assert!(matches!(world.get::<Option<&Foo>, _>(&e_owning), Some(Foo)));

    // Fetching `&Foo` without `Option` won't compile as we don't have
    // a proof that `Foo` component is present on the entity yet.
    // To get this proof components must be pinned to the entity.
    //
    // At the time of writing, pinning components only verifies that entity has specified components.
    // At any later point of time, it may be possible to remove pinned component.
    // This results in contract breaking of pins. And may end up with `World::get` panic.
    let e_with_foo: Entity<(Foo,)> = e_owning.pin(&mut world);

    // Now it is possible to fetch `Foo` without `Result` or `Option`.
    assert!(matches!(world.get::<&Foo, _>(&e_with_foo), Foo));

    // Owning entity can be asked for `id` to work with it like with
    // `World` owned entity.
    let e = e_with_foo.id();

    // To check if entity is alive, user may call `World::is_alive`.
    assert!(world.is_alive(&e));

    // User may query components using `EntityId`.
    // Doing so requires using fallible counterparts of `get` and `get_mut`,
    // namely `query_one` and `query_mut`.
    assert!(matches!(world.query_one::<&Foo>(&e), Ok(Foo)));

    // `Entity` dereferences to `EntityId`
    // allowing using `Entity` whenever `EntityId` is expected.
    let _res = world.query_one::<&Foo>(&e_with_foo);

    // Dropping owning references causes entity to be despawned.
    drop(e_with_foo);

    // But not immediately.
    assert!(world.is_alive(&e));

    // This call causes all deferred operations to complete.
    // Requires mutable reference to `World`, so no other operations with components and entities
    // can happen in parallel.
    //
    // As for now, the only deferred operation is entity cleanup,
    // that despawns all entities to which no strong references left.
    world.maintain();

    // Now entity is despawned
    assert!(!world.is_alive(&e));

    // Query reports the same.
    assert!(matches!(
        world.query_one::<&Foo>(&e),
        Err(QueryOneError::NoSuchEntity)
    ));

    // Owning reference can be obtained immediately on spawn
    // using `World::spawn_owning`
    let e_owning = world.spawn_owning((Foo, Bar, Baz));

    let e = e_owning.id();

    // Owning reference can be converted to shared reference.
    let e_shared = e_owning.share();

    // Shared references can be cloned.
    // This costs one atomic operation, same as for `Arc` clone.
    let e_clone = e.clone();

    // After dropping strong entity, reference counter decrements,
    // but there is still a clone of `e` alive.
    drop(e_shared);

    // Dropping last strong reference to an entity.
    drop(e_clone);

    world.maintain();

    // The entity is gone.
    assert!(!world.is_alive(&e));

    // Entities can be spawned in batches using iterators.
    // Each iterator element is treated as bundle of components
    // and spawned entities receive them.
    //
    // This is more efficient than spawning in loop,
    // especially if iterator size hints are more or less accurate
    // and not `(0, None)`
    //
    // `World::spawn_batch` returns an iterator with `Entity` for each entity created.
    let _entities: Vec<_> = world
        .spawn_batch_owning((0..10u32).map(|i| (Value(i),)))
        .collect();

    // User may choose to not consume returned iterator, or consume it partially.
    // This would cause original iterator to not be consumed as well and entities will not be spawned.
    //
    // This allows using unbound iterators to produce entities and stop at any moment.
    // Note that above version is better as it offers chance to reserve space for entities.
    let _entities: Vec<_> = world
        .spawn_batch_owning((0u32..).map(|i| (Value(i),)))
        .take(10)
        .collect();

    // // Ref-counting allows defining entity ownership relations
    // let parent = world.spawn((Parent {
    //     children: Vec::new(),
    // },));

    // let children = world
    //     .spawn_batch_owning(std::iter::repeat((Child,)).take(10))
    //     .collect::<Vec<_>>();

    // let weak_children = children.iter().map(|e| **e).collect::<Vec<_>>();

    // world
    //     .query_one_mut::<&mut Parent>(&parent)
    //     .unwrap()
    //     .children = children;

    // // Now `parent` owns the children entities.
    // // When `parent` is despawned, children loose one strong reference,
    // // if it was the last one they are despawned as well.
    // world.despawn(&parent).unwrap();
    // world.maintain();

    // for child in weak_children {
    //     assert!(!world.is_alive(&child));
    // }
}
