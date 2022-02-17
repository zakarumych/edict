use crate::{
    query::Modifed,
    world::{EntityError, World},
};

/// Tests that entity spawned into world has all components from bundle.
#[test]
fn world_spawn() {
    let mut world = World::new();

    let e = world.spawn((42u32, "qwe"));
    assert_eq!(world.has_component::<u32>(&e), Ok(true));
    assert_eq!(world.has_component::<&str>(&e), Ok(true));
    assert_eq!(world.query_one_mut::<(&u32, &&str)>(&e), Ok((&42, &"qwe")));
}

/// Tests that entity does not have a component that wasn't in spawn bundle
/// but has it after component is inserted
#[test]
fn world_insert() {
    let mut world = World::new();

    let e = world.spawn((42u32,));
    assert_eq!(world.has_component::<u32>(&e), Ok(true));
    assert_eq!(world.has_component::<&str>(&e), Ok(false));
    assert_eq!(
        world.query_one_mut::<(&u32, &&str)>(&e),
        Err(EntityError::MissingComponents)
    );

    assert_eq!(world.try_insert(&e, "qwe"), Ok(()));
    assert_eq!(world.has_component::<&str>(&e), Ok(true));
    assert_eq!(world.query_one_mut::<(&u32, &&str)>(&e), Ok((&42, &"qwe")));
}

/// Tests that entity does not have a component that was removed.
#[test]
fn world_remove() {
    let mut world = World::new();

    let e = world.spawn((42u32, "qwe"));
    assert_eq!(world.has_component::<u32>(&e), Ok(true));
    assert_eq!(world.has_component::<&str>(&e), Ok(true));
    assert_eq!(world.query_one_mut::<(&u32, &&str)>(&e), Ok((&42, &"qwe")));

    assert_eq!(world.remove::<&str>(&e), Ok("qwe"));
    assert_eq!(world.has_component::<&str>(&e), Ok(false));
    assert_eq!(
        world.query_one_mut::<(&u32, &&str)>(&e),
        Err(EntityError::MissingComponents)
    );
}

/// Insertion test. Bundle version
#[test]
fn world_insert_bundle() {
    let mut world = World::new();

    let e = world.spawn((42u32,));
    assert_eq!(world.has_component::<u32>(&e), Ok(true));
    assert_eq!(world.has_component::<&str>(&e), Ok(false));
    assert_eq!(
        world.query_one_mut::<(&u32, &&str)>(&e),
        Err(EntityError::MissingComponents)
    );

    assert_eq!(world.try_insert_bundle(&e, ("qwe", true)), Ok(()));
    assert_eq!(world.has_component::<&str>(&e), Ok(true));
    assert_eq!(world.has_component::<bool>(&e), Ok(true));
    assert_eq!(
        world.query_one_mut::<(&u32, &&str, &bool)>(&e),
        Ok((&42, &"qwe", &true))
    );
}

/// Removing test. Bundle version.
#[test]
fn world_remove_bundle() {
    let mut world = World::new();

    let e = world.spawn((42u32, "qwe"));
    assert_eq!(world.has_component::<u32>(&e), Ok(true));
    assert_eq!(world.has_component::<&str>(&e), Ok(true));
    assert_eq!(world.query_one_mut::<(&u32, &&str)>(&e), Ok((&42, &"qwe")));

    // When removing a bundle, any missing component is simply ignored.
    assert_eq!(world.remove_bundle::<(&str, bool)>(&e), Ok(()));
    assert_eq!(world.has_component::<&str>(&e), Ok(false));
    assert_eq!(
        world.query_one_mut::<(&u32, &&str)>(&e),
        Err(EntityError::MissingComponents)
    );
}

#[test]
fn version_test() {
    let mut world = World::new();

    let mut tracks = world.tracks();
    let e = world.spawn((42u32, "qwe"));

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![(e, &42)]
    );

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![]
    );

    *world.query_one_mut::<&mut u32>(&e).unwrap() = 42;

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![(e, &42)]
    );
}

#[test]
fn version_despawn_test() {
    let mut world = World::new();

    let mut tracks = world.tracks();
    let e1 = world.spawn((42u32, "qwe"));
    let e2 = world.spawn((23u32, "rty"));

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![(e1, &42), (e2, &23)]
    );

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![]
    );

    *world.query_one_mut::<&mut u32>(&e2).unwrap() = 50;
    assert_eq!(world.despawn(&e1), Ok(()));

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![(e2, &50)]
    );
}

#[test]
fn version_insert_test() {
    let mut world = World::new();

    let mut tracks = world.tracks();
    let e1 = world.spawn((42u32, "qwe"));
    let e2 = world.spawn((23u32, "rty"));

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![(e1, &42), (e2, &23)]
    );

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![]
    );

    *world.query_one_mut::<&mut u32>(&e1).unwrap() = 50;
    *world.query_one_mut::<&mut u32>(&e2).unwrap() = 100;

    assert_eq!(world.try_insert(&e1, true), Ok(()));

    assert_eq!(
        world
            .query::<Modifed<&u32>>()
            .tracked_iter(&mut tracks)
            .collect::<Vec<_>>(),
        vec![(e2, &100), (e1, &50)]
    );
}
