//! This example shows how to use entity reserving mechanism.

use edict::{
    action::ActionEncoder, component::Component, entity::EntityLoc, scheduler::Scheduler,
    world::World,
};

#[derive(Component)]
pub struct Foo;

fn main() {
    let mut world = World::new();
    let mut scheduler = Scheduler::new();

    scheduler.add_system(allocate_system);
    scheduler.add_system(spawn_system);

    scheduler.run_sequential(&mut world);
    scheduler.run_sequential(&mut world);

    assert_eq!(world.view::<&Foo>().iter().count(), 4);
}

fn allocate_system(world: &World, mut encoder: ActionEncoder) {
    let entity = world.allocate();
    encoder.insert(entity, Foo);
}

fn spawn_system(mut encoder: ActionEncoder) {
    let _id: EntityLoc = encoder.spawn((Foo,));
}
