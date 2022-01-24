//! This example contains usage of the main features of Edict ECS.
use edict::prelude::*;

/// Just a type.
/// Being `'static` makes it a proper component type.
struct Foo;

/// Another type.
struct Bar;

/// Another type.
struct Baz;

/// Parent component type.
struct Parent {
    children: Vec<Entity>,
}

/// Child component type.
struct Child {
    parent: EntityId,
}

fn main() {
    // Create new World.
    let mut world = World::new();
    // It doesn not contain any entities yet.

    // Spawn new entity in the world and give it three components,
    // namely `Foo`, `Bar` and `Baz` components.
    // `World::spawn` takes a `Bundle` implementation.
    // Tuples of various size implement `Bundle` trait.
    // Using this method with tuple will cause all tuple elements
    // to be added as components to the entity.
    //
    // Take care to now try to add duplicate components in one bundle
    // as method will surely panic.
    let e = world.spawn_owned((Foo, Bar, Baz));

    // Entity can be used to access components in the `World`.
    // Note that we query entity for `Option<&Foo>`,
    // because we have no proof entity has one.
    // Querying `&Foo` would not compile. More on this later.
    let _foo = world.get::<Option<&Foo>, _>(&e);

    // To add another component to the entitiy call `World::insert`.
    world.insert(&e, 0u32);
    assert_eq!(world.get::<Option<&u32>, _>(&e), Some(&0));

    // If the component is already present in entity, the value is simply replaced.
    world.insert(&e, 1u32);
    assert_eq!(world.get::<Option<&u32>, _>(&e), Some(&1));

    // To add few components at once user should call `World::insert_bundle`.
    // This is much more efficient than adding components once by one.
    world.insert_bundle(&e, (1u8, 2u16));

    // `Entity` can asked for its `EntityId`.
    // While `Entity` acts like strong reference to the entity in the world,
    // keeping enitity and its components alive,
    // `EntityId` acts like weak reference and may become obsolete
    // if referenced entity is dropped.
    let e_weak = e.id();

    // To check if entity is alive, user may call `World::is_alive`.
    assert!(world.is_alive(e_weak), "We still hold `e: Entity`");

    // User may query components using `EntityId`.
    // Doing so requires using fallible counterparts of `get` and `get_mut`,
    // namely `query_one` and `query_mut`.
    match world.query_one::<&Foo>(e_weak) {
        Err(EntityError::NoSuchEntity) => {
            // If entity was despawned already this error variant is returned.
            unreachable!("We still hold `e: Entity`");
        }
        Err(EntityError::MissingComponents) => {
            // If entity does not have all components required for query to succeed
            // this error variant is returned.
            unreachable!("We've put `Foo` there");
        }
        Ok(_foo) => {
            // Here we got the query item.
        }
    }

    // `Entity` dereferences to `EntityId`
    // allowing using `Entity` whenever `EntityId` is expected.
    let _res = world.query_one::<&Foo>(*e);

    // Entities can be cloned.
    // This costs one atomic operation, same as for `Arc` clone.
    let e_clone = e.clone();

    // After dropping stong entity, reference counter decrements,
    // but there is still a clone of `e` alive.
    drop(e);

    // Dropping last stong reference to an entity.
    drop(e_clone);

    // Although last strong reference to an entity was dropped,
    // the entity is still alive. When would it be despawned?
    assert!(world.is_alive(e_weak), "Still alive. But not for long");

    // Queries still work as before.
    assert!(
        matches!(world.query_one::<&Foo>(e_weak), Ok(Foo)),
        "Components are still there too"
    );

    // This call causes all deferred operations to complete.
    // Requires mutable reference to `World`, so no other operations with components and entities
    // can happen in parallel.
    //
    // As for now, the only deferred operation is entity cleaup,
    // that despawns all enitites to which no strong references left.
    world.maintain();

    // The entity is gone.
    assert!(!world.is_alive(e_weak), "Finally despawned");

    // Query reports the same.
    assert!(
        matches!(
            world.query_one::<&Foo>(e_weak),
            Err(EntityError::NoSuchEntity)
        ),
        "No such entity"
    );

    // Entities can be spawned in batches using iterators.
    // Each iterator element is treated as bundle of components
    // and spawned entities receive them.
    //
    // This is more efficient than spawning in loop,
    // especially if iterator size hints are more or less accurate
    // and not `(0, None)`
    //
    // `World::spawn_batch` returns an iterator with `Entity` for each entity created.
    let _entities: Vec<_> = world.spawn_batch_owned((0..10u32).map(|i| (i,))).collect();

    // User may choose to not consume returned iterator, or consume it partially.
    // This would cause original iterator to not be consumed as well and entities will not be spawned.
    //
    // This allows using unbound iterators to produce entities and stop at any moment.
    // Note that above version is better as it offers chance to reserve space for entities.
    let _entities: Vec<_> = world
        .spawn_batch_owned((0u32..).map(|i| (i,)))
        .take(10)
        .collect();

    // Refcounting allows defining entity ownership relations
    let parent = world.spawn((Parent {
        children: Vec::new(),
    },));

    let children = world
        .spawn_batch_owned((0..10).map(|_| (Child { parent },)))
        .collect::<Vec<_>>();

    let weak_children = children.iter().map(|e| **e).collect::<Vec<_>>();

    world.query_one_mut::<&mut Parent>(parent).unwrap().children = children;

    // Now `parent` owns the children entities.
    // When `parent` is despanwed, children loose one strong reference,
    // if it was the last one they are despawned as well.
    world.despawn(parent).unwrap();
    world.maintain();

    for child in weak_children {
        assert!(!world.is_alive(child));
    }
}
