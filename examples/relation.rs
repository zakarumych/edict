use edict::{
    component::Component,
    epoch::EpochId,
    query::Entities,
    relation::Relation,
    relation::{relates_to, Relates},
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

    let a = world.spawn((A,));
    let b = world.spawn(());

    world.add_relation(a, ChildOf, b).unwrap();

    for (e, ChildOf) in world.query::<Entities>().relates_to::<&ChildOf>(b).iter() {
        println!("{} is child of {}", e, b);
    }

    world.despawn(b).unwrap();

    assert_eq!(world.is_alive(a), false);

    let a = world.spawn(());
    let b = world.spawn(());
    let c = world.spawn(());

    world.add_relation(a, Likes, b).unwrap();
    world.add_relation(a, Likes, c).unwrap();

    assert!(world.query_one_with(a, relates_to::<Likes>(b)).is_ok());
    assert!(world.query_one_with(a, relates_to::<Likes>(c)).is_ok());

    assert_eq!(
        world
            .query_one::<Relates<&Likes>>(a)
            .unwrap()
            .clone()
            .collect::<Vec<_>>(),
        vec![(&Likes, b), (&Likes, c)]
    );

    world.despawn(b).unwrap();

    assert_eq!(
        world
            .query_one::<Relates<&Likes>>(a)
            .unwrap()
            .clone()
            .collect::<Vec<_>>(),
        vec![(&Likes, c)]
    );

    let b = world.spawn(());

    world.add_relation(a, Enemy, b).unwrap();

    let q = world.query::<Entities>().relates::<&Enemy>();
    for (e, enemies) in q.iter() {
        println!(
            "{} is enemy of {:?}",
            e,
            enemies.into_iter().collect::<Vec<_>>()
        );
    }
    drop(q);

    let _ = world.despawn(b);

    for (e, enemies) in world.query::<Entities>().relates::<&Enemy>().iter() {
        println!(
            "{} is enemy of {:?}",
            e,
            enemies.into_iter().collect::<Vec<_>>()
        );
    }

    let since = EpochId::start();

    let mut query = world
        .query::<(Entities, &A)>()
        .with::<B>()
        .modified::<&C>(since)
        .relates_to::<&ChildOf>(b)
        .filter(relates_to::<Likes>(c));

    for ((e, a), c, child_of) in query.iter_mut() {
        drop((e, a, c, child_of));
    }
    drop(query);

    let a = world.spawn((A,));
    let b = world.spawn((B,));

    world.add_relation(a, LifeBound, b).unwrap();

    world.despawn(a).unwrap();
    assert_eq!(world.is_alive(b), false);
}
