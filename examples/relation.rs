use edict::{
    action::ActionEncoder,
    component::Component,
    entity::EntityId,
    epoch::EpochId,
    relation::Relation,
    relation::{QueryRelation, WithRelationTo},
    world::WorldBuilder,
};

struct A;

impl Component for A {}

struct B;

impl Component for B {}

struct C;

impl Component for C {}

#[derive(Clone, Copy)]
struct ChildOf;

impl Relation for ChildOf {
    const EXCLUSIVE: bool = true;

    fn on_target_drop(entity: EntityId, _target: EntityId, encoder: &mut ActionEncoder) {
        encoder.despawn(entity);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Likes;

impl Relation for Likes {}

#[derive(Clone, Copy, Debug)]
struct Enemy;

impl Relation for Enemy {
    const SYMMETRIC: bool = true;
}

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

    for (e, ChildOf) in world.build_query().relation_to::<&ChildOf>(b) {
        println!("{} is child of {}", e, b);
    }

    world.despawn(b).unwrap();

    assert_eq!(world.is_alive(a), false);

    let a = world.spawn(());
    let b = world.spawn(());
    let c = world.spawn(());

    world.add_relation(a, Likes, b).unwrap();
    world.add_relation(a, Likes, c).unwrap();

    assert_eq!(
        world.query_one_state(a, WithRelationTo::<Likes>::new(b)),
        Ok(())
    );
    assert_eq!(
        world.query_one_state(a, WithRelationTo::<Likes>::new(c)),
        Ok(())
    );
    assert_eq!(
        world
            .query_one::<QueryRelation<&Likes>>(a)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![(&Likes, b), (&Likes, c)]
    );

    world.despawn(b).unwrap();

    assert_eq!(
        world
            .query_one::<QueryRelation<&Likes>>(a)
            .unwrap()
            .collect::<Vec<_>>(),
        vec![(&Likes, c)]
    );

    let b = world.spawn(());

    world.add_relation(a, Enemy, b).unwrap();

    let q = world.build_query().relation::<&Enemy>();
    for (e, enemies) in q {
        println!(
            "{} is enemy of {:?}",
            e,
            enemies.into_iter().collect::<Vec<_>>()
        );
    }

    let _ = world.despawn(b);

    for (e, enemies) in world.build_query().relation::<&Enemy>() {
        println!(
            "{} is enemy of {:?}",
            e,
            enemies.into_iter().collect::<Vec<_>>()
        );
    }

    let since = EpochId::start();

    let query = world
        .query::<&A>()
        .with::<B>()
        .modified::<&C>(since)
        .relation_to::<&ChildOf>(b)
        .with_relation_to::<Likes>(c);

    for (e, (a, c, child_of)) in query {
        drop((e, a, c, child_of));
    }
}
