use edict::{component::Component, query::Entities, world::World};

#[derive(Debug, Component)]
struct A;

#[derive(Debug, Component)]
struct B;

fn main() {
    let mut world = World::new();

    world.spawn((A, B));

    let bs = world.view::<&B>();

    for (e, a) in world.view::<(Entities, &A)>() {
        if let Some(b) = bs.get(e) {
            println!("A {:?} has B {:?}", a, b);
        }
    }
}
