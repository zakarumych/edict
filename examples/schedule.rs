use core::fmt::Debug;

use edict::{
    executor::MockExecutor,
    query::QueryBorrowAll,
    scheduler::Scheduler,
    system::State,
    world::{QueryRef, World},
};
use edict_proc::Component;

#[derive(Clone, Copy, Component)]
struct A;

#[derive(Clone, Copy, Debug, Component)]
#[edict(borrow(dyn Debug))]
struct B;

fn main() {
    let mut world = World::new();
    let mut schedule = Scheduler::new();

    world.spawn((A,));
    world.spawn((A, B));
    world.spawn((B,));

    schedule.add_system(system_a);

    for _ in 0..10 {
        schedule.run(&mut world, &MockExecutor);
    }
}

fn system_a(
    q: QueryRef<(
        &A,
        Option<&B>,
        Option<QueryBorrowAll<&(dyn Debug + Sync + 'static)>>,
    )>,
    mut counter: State<u32>,
) {
    *counter += 1;
    println!("{}", *counter);
    for (_, (&A, b, dbg)) in q {
        println!("A + {:?} + {:?}", b, dbg);
    }
}
