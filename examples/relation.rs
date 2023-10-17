use edict::{
    component::Component,
    epoch::EpochId,
    query::Entities,
    relation::{Relates, Relation},
    world::WorldBuilder,
};

#[derive(Component)]
struct A;

#[derive(Component)]
struct B;

#[derive(Component)]
struct C;

#[derive(Clone, Copy, Relation)]
#[edict(exclusive, owned)]
struct ChildOf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Relation)]
struct Likes;

#[derive(Clone, Copy, Debug, Relation)]
#[edict(symmetric)]
struct Enemy;

#[derive(Clone, Copy, Debug, Relation)]
#[edict(symmetric, owned)]
struct LifeBound;

fn main() {
    let mut world_builder = WorldBuilder::new();
    world_builder
        .register_component::<A>()
        .on_drop_fn(|a, entity, encoder| {
            a.on_drop(entity, encoder);
            println!("A dropped");
        });

    let mut world = world_builder.build();

    let a = world.spawn((A,)).id();
    let b = world.spawn(()).id();

    world.add_relation(a, ChildOf, b).unwrap();

    for (e, ChildOf) in world.view::<Entities>().relates_to::<ChildOf>(b).iter() {
        println!("{} is child of {}", e, b);
    }

    world.despawn(b).unwrap();

    assert_eq!(world.is_alive(a), false);

    let a = world.spawn(()).id();
    let b = world.spawn(()).id();
    let c = world.spawn(()).id();

    world.add_relation(a, Likes, b).unwrap();
    world.add_relation(a, Likes, c).unwrap();

    let mut view = world.get::<Relates<&Likes>>(a).unwrap();
    let first = view.next().unwrap();
    assert_eq!(first.1, b);
    let second = view.next().unwrap();
    assert_eq!(second.1, c);

    world.despawn(b).unwrap();

    let mut view = world.get::<Relates<&Likes>>(a).unwrap();
    let first = view.next().unwrap();
    assert_eq!(first.1, c);

    let b = world.spawn(()).id();

    world.add_relation(a, Enemy, b).unwrap();

    let q = world.view::<Entities>().relates::<Enemy>();
    for (e, enemies) in q.iter() {
        println!(
            "{} is enemy of {:?}",
            e,
            enemies.into_iter().collect::<Vec<_>>()
        );
    }
    drop(q);

    let _ = world.despawn(b);

    for (e, enemies) in world.view_mut::<Entities>().relates::<Enemy>() {
        println!(
            "{} is enemy of {:?}",
            e,
            enemies.into_iter().collect::<Vec<_>>()
        );
    }

    let since = EpochId::start();

    let view = world
        .view_mut::<(Entities, &A)>()
        .with::<B>()
        .modified::<&C>(since)
        .relates_to::<ChildOf>(b)
        .filter_relates_to::<Likes>(c);

    for ((e, a), c, child_of) in view {
        let _ = (e, a, c, child_of);
    }

    let a = world.spawn((A,)).id();
    let b = world.spawn((B,)).id();

    world.add_relation(a, LifeBound, b).unwrap();

    world.despawn(a).unwrap();
    assert_eq!(world.is_alive(b), false);
}
