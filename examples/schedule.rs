use std::fmt::Debug;

use edict::{
    query::{Modified, QueryBorrowAll, With, Without},
    scheduler::Scheduler,
    system::State,
    view::{View, ViewCell},
    world::World,
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
    let c = world.spawn((B,)).id();

    schedule.add_system(system_a);
    schedule.add_system(system_b);
    schedule.add_system(system_c);
    schedule.add_system(system_d);

    for i in 0..10 {
        println!("Loop: {i}");

        world.get::<&mut B>(c).unwrap();

        schedule.run_threaded(&mut world);
    }
}

fn system_a(
    // `ViewCell` is required because `&mut A` conflicts with `QueryBorrowAll`
    view: ViewCell<(
        &mut A,
        Option<&B>,
        Option<QueryBorrowAll<&(dyn Debug + Sync + 'static)>>,
    )>,
    mut counter: State<u32>,
) {
    *counter += 1;
    println!("Counter: {}", *counter);
    for (&mut A, b, dbg) in view.iter() {
        println!("A + {:?} + {:?}", b, dbg);
    }
}

fn system_b(q: View<Modified<&B>>) {
    for &B in q.iter() {
        println!("Modified B");
    }
}

fn system_c(v: View<&mut A>) {
    v.into_iter().for_each(|_| {});
}

// `ViewCell` is required because v1 conflicts with v2
fn system_d(v1: View<&mut A, With<B>>, v2: View<&mut A, Without<B>>) {
    v1.into_iter().for_each(|_| {});
    v2.into_iter().for_each(|_| {});
}
