//! This example shows how to use entity reserving mechanism.

use edict::{
    action::ActionEncoder, executor::MockExecutor, prelude::ActionEncoderSliceExt,
    scheduler::Scheduler, world::World,
};
use edict_proc::Component;

#[derive(Component)]
pub struct Foo;

fn main() {
    let mut world = World::new();
    let mut scheduler = Scheduler::new();

    scheduler.add_system(reserve_system);

    scheduler
        .run(&mut world, &MockExecutor)
        .execute_all(&mut world);
    scheduler
        .run(&mut world, &MockExecutor)
        .execute_all(&mut world);
    scheduler
        .run(&mut world, &MockExecutor)
        .execute_all(&mut world);
    scheduler
        .run(&mut world, &MockExecutor)
        .execute_all(&mut world);

    assert_eq!(world.query::<&Foo>().into_iter().count(), 4);
}

fn reserve_system(world: &World, encoder: &mut ActionEncoder) {
    let entity = world.reserve();
    encoder.insert(entity, Foo);
}
