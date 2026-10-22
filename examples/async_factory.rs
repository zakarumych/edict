//! Flow-driven orders share a deterministic factory with ordinary ECS systems.
//!
//! Run with `cargo run --example async_factory`. A tick has two phases:
//! scheduled production and deferred actions, then polling explicitly ready flows.

use edict::{
    action::{ActionEncoder, LocalActionEncoder},
    component::Component,
    entity::EntityId,
    flow::{FlowEntity, Flows},
    query::Entities,
    relation::ChildOf,
    resources::{Res, ResMut},
    scheduler::Scheduler,
    system::State,
    view::View,
    world::World,
};
use std::{
    sync::{Arc, Mutex},
    task::{Poll, Waker},
};

#[derive(Clone, Default)]
struct Timeline(Arc<Mutex<Vec<String>>>);
impl Timeline {
    fn record(&self, event: impl Into<String>) {
        self.0.lock().unwrap().push(event.into());
    }
}
#[derive(Component, Clone, Copy)]
struct Order {
    number: u32,
    item: &'static str,
    duration: u32,
}
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Queued,
    Processing,
    Completed,
}
#[derive(Component)]
struct Admitted;
#[derive(Component)]
struct Processing {
    remaining: u32,
    done: bool,
    wakers: Vec<Waker>,
}
struct OrderAudit(u32);
impl Component for OrderAudit {
    fn on_drop(&mut self, _: EntityId, mut actions: LocalActionEncoder) {
        let number = self.0;
        actions.closure(move |world| {
            world
                .expect_resource::<Timeline>()
                .record(format!("order #{number}: owned entity removed"))
        });
    }
}
#[derive(Default)]
struct Materials {
    raw: u32,
    admitted: u32,
}
#[derive(Default)]
struct Shipped(Vec<u32>);
#[derive(Default)]
struct Clock(u32);
struct Factory(EntityId);
#[derive(Clone, Default)]
struct Outcomes(Arc<Mutex<Vec<(u32, bool)>>>);

// The guard lives inside each async task, so cancellation has a visible audit.
struct TaskAudit {
    number: u32,
    finished: bool,
    outcomes: Outcomes,
    timeline: Timeline,
}
impl Drop for TaskAudit {
    fn drop(&mut self) {
        self.outcomes
            .0
            .lock()
            .unwrap()
            .push((self.number, self.finished));
        if !self.finished {
            self.timeline.record(format!(
                "order #{}: flow cancelled; unfinished work released",
                self.number
            ));
        }
    }
}

fn tick(mut clock: ResMut<Clock>, mut elapsed: State<u32>) {
    *elapsed += 1;
    clock.0 = *elapsed;
}
fn admit(
    orders: View<(Entities, &Order, &Stage)>,
    mut materials: ResMut<Materials>,
    timeline: Res<Timeline>,
    mut actions: ActionEncoder,
) {
    for (entity, order, stage) in orders {
        if *stage != Stage::Queued || materials.raw == 0 {
            continue;
        }
        materials.raw -= 1;
        materials.admitted += 1;
        actions.insert(entity, Admitted);
        timeline.record(format!(
            "order #{}: admitted one material unit for {}",
            order.number, order.item
        ));
    }
}
fn produce(jobs: View<(&Order, &mut Processing)>, timeline: Res<Timeline>) {
    for (order, job) in jobs {
        if job.done {
            continue;
        }
        job.remaining -= 1;
        timeline.record(format!(
            "order #{}: machine step; {} remaining",
            order.number, job.remaining
        ));
        if job.remaining == 0 {
            job.done = true;
            // Explicit wakeup reconnects the production system with its waiting flow.
            for waker in job.wakers.drain(..) {
                waker.wake();
            }
        }
    }
}
fn ship(orders: View<(&Order, &Stage)>, mut shipped: ResMut<Shipped>, timeline: Res<Timeline>) {
    for (order, stage) in orders {
        if *stage == Stage::Completed && !shipped.0.contains(&order.number) {
            shipped.0.push(order.number);
            timeline.record(format!("order #{}: shipped {}", order.number, order.item));
        }
    }
}
fn close(
    clock: Res<Clock>,
    factory: Res<Factory>,
    timeline: Res<Timeline>,
    mut actions: ActionEncoder,
) {
    if clock.0 == 6 {
        timeline.record("factory shutdown: despawn parent and cancel unfinished orders");
        actions.despawn(factory.0);
    }
}

async fn fulfill(entity: FlowEntity, order: Order, mut audit: TaskAudit) {
    entity.wait_has_component::<Admitted>().await;
    audit
        .timeline
        .record(format!("order #{}: queued -> processing", order.number));
    entity.insert(Stage::Processing);
    entity.insert(Processing {
        remaining: order.duration,
        done: false,
        wakers: vec![],
    });
    entity
        .poll(|mut entity, cx| {
            let job = entity
                .get_mut::<&mut Processing>()
                .expect("processing component exists while order is alive");
            if job.done {
                return Poll::Ready(());
            }
            if !job.wakers.iter().any(|waker| waker.will_wake(cx.waker())) {
                job.wakers.push(cx.waker().clone());
            }
            Poll::Pending
        })
        .await;
    entity.insert(Stage::Completed);
    audit.finished = true;
    audit
        .timeline
        .record(format!("order #{}: processing -> completed", order.number));
}

fn main() {
    let mut world = World::new();
    let mut flows = Flows::new();
    let timeline = Timeline::default();
    let outcomes = Outcomes::default();
    world.insert_resource(timeline.clone());
    world.insert_resource(Clock::default());
    world.insert_resource(Materials {
        raw: 3,
        admitted: 0,
    });
    world.insert_resource(Shipped::default());
    let factory = world.spawn(()).id();
    world.insert_resource(Factory(factory));
    let mut order_ids = Vec::new();
    for order in [
        Order {
            number: 1,
            item: "sensor",
            duration: 2,
        },
        Order {
            number: 2,
            item: "motor",
            duration: 4,
        },
        Order {
            number: 3,
            item: "reactor",
            duration: 7,
        },
    ] {
        let entity = world
            .spawn((order, Stage::Queued, OrderAudit(order.number)))
            .id();
        world.insert_relation(entity, ChildOf, factory).unwrap();
        order_ids.push(entity);
        let audit = TaskAudit {
            number: order.number,
            finished: false,
            outcomes: outcomes.clone(),
            timeline: timeline.clone(),
        };
        world.spawn_flow_for(entity, move |entity| fulfill(entity, order, audit));
        timeline.record(format!(
            "order #{}: queued {} ({} machine ticks)",
            order.number, order.item, order.duration
        ));
    }
    let mut schedule = Scheduler::new();
    schedule.add_system(tick);
    schedule.add_system(admit);
    schedule.add_system(produce);
    schedule.add_system(ship);
    schedule.add_system(close);

    println!("ASYNC FACTORY / schedule -> apply actions -> poll flows");
    flows.execute(&mut world); // Register component waits before admitting work.
    let mut printed = 0;
    for step in 1..=6 {
        timeline.record(format!("--- tick {step}: scheduled production ---"));
        schedule.run_sequential(&mut world);
        timeline.record(format!("--- tick {step}: ready flows ---"));
        flows.execute(&mut world);
        if step == 1 {
            assert!(
                order_ids
                    .iter()
                    .all(|id| world.get::<&Stage>(*id).unwrap() == &Stage::Processing)
            );
            assert!(outcomes.0.lock().unwrap().is_empty());
        }
        if step == 3 {
            assert_eq!(outcomes.0.lock().unwrap().as_slice(), &[(1, true)]);
        }
        if step == 5 {
            assert_eq!(
                outcomes.0.lock().unwrap().as_slice(),
                &[(1, true), (2, true)]
            );
        }
        let events = timeline.0.lock().unwrap();
        for event in &events[printed..] {
            println!("  {event}");
        }
        printed = events.len();
    }
    let mut result = outcomes.0.lock().unwrap().clone();
    result.sort();
    assert_eq!(result, vec![(1, true), (2, true), (3, false)]);
    assert_eq!(world.expect_resource::<Shipped>().0, vec![1, 2]);
    assert_eq!(world.expect_resource::<Materials>().raw, 0);
    assert_eq!(world.expect_resource::<Materials>().admitted, 3);
    assert_eq!(world.view::<Entities>().iter().count(), 0);
    println!(
        "\nResult: 3 materials admitted, 2 products shipped, 1 unfinished order cancelled; no entities remain."
    );
}
