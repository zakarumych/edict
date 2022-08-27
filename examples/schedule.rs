use core::fmt::Debug;

use edict::{
    query::{Modified, QueryBorrowAll, With},
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
    let c = world.spawn((B,));

    schedule.add_system(system_a);
    schedule.add_system(system_b);
    schedule.add_system(system_c);
    schedule.add_system(system_d);
    schedule.add_system(system_e);

    for i in 0..10 {
        println!("Loop: {i}");

        world.query_one::<&mut B>(c).unwrap();

        std::thread::scope(|scope| {
            schedule.run(&mut world, &scope);
        });
    }
}

fn system_a(
    q: QueryRef<(
        &mut A,
        Option<&B>,
        Option<QueryBorrowAll<&(dyn Debug + Sync + 'static)>>,
    )>,
    mut counter: State<u32>,
) {
    *counter += 1;
    println!("Counter: {}", *counter);
    for (_, (&mut A, b, dbg)) in q {
        println!("A + {:?} + {:?}", b, dbg);
    }
}

fn system_b(q: QueryRef<Modified<&B>>) {
    for (_, &B) in q {
        println!("Modified B");
    }
}

fn system_c(q: QueryRef<&mut A>) {
    q.for_each(|_| {});
}
fn system_d(q: QueryRef<&mut A, With<B>>) {
    q.for_each(|_| {});
}
fn system_e(q: QueryRef<&A, With<B>>) {
    q.for_each(|_| {});
}
