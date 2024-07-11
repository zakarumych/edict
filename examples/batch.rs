use edict::prelude::*;

#[derive(Debug, Component)]
struct A;

#[derive(Debug, Component)]
struct B;

fn main() {
    let mut world = World::new();

    world.spawn_batch((0..256).map(|_| (A, B))).spawn_all();

    for (a, b) in world.view::<(&A, &B)>().into_iter_batched(32) {
        for (a, b) in a.iter().zip(b.iter()) {
            println!("{:?} {:?}", a, b);
        }
    }
}
