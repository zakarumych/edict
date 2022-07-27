use edict::{
    action::ActionEncoder,
    entity::EntityId,
    prelude::{Component, World},
    query::Related,
    relation::Relation,
};

#[derive(Clone, Copy)]
struct ChildOf;

impl Relation for ChildOf {
    const EXCLUSIVE: bool = true;

    fn on_target_drop(entity: EntityId, _target: EntityId, encoder: &mut ActionEncoder) {
        encoder.despawn(entity);
    }
}

struct A;

impl Component for A {}

fn main() {
    let mut world = World::new();

    let a = world.spawn(());
    let b = world.spawn(());

    world.try_add_relation(&a, ChildOf, &b).unwrap();

    for (e, child) in world.query::<Related<&ChildOf>>().iter() {
        for (ChildOf, parent) in child {
            println!("{} is child of {}", e, parent);
        }
    }

    world.despawn(&b).unwrap();

    assert_eq!(world.is_alive(&a), false);
}
