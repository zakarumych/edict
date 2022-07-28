use edict::{
    action::ActionEncoder,
    entity::EntityId,
    prelude::{Component, World},
    relation::Relation,
};

struct A;

impl Component for A {}

#[derive(Clone, Copy)]
struct ChildOf;

impl Relation for ChildOf {
    const EXCLUSIVE: bool = true;

    fn on_target_drop(entity: EntityId, _target: EntityId, encoder: &mut ActionEncoder) {
        encoder.despawn(entity);
    }
}

fn main() {
    let mut world = World::new();

    let a = world.spawn(());
    let b = world.spawn(());

    world.try_add_relation(&a, ChildOf, &b).unwrap();

    for (e, ChildOf) in world.build_query().relation_to::<&ChildOf>(b) {
        println!("{} is child of {}", e, b);
    }

    world.despawn(&b).unwrap();

    assert_eq!(world.is_alive(&a), false);
}
