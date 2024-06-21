use std::task::{Poll, Waker};

use edict::{
    action::ActionEncoder,
    component::Component,
    flow::{flow_fn, Entity, Flows},
    query::Entities,
    resources::Res,
    scheduler::Scheduler,
    view::View,
    world::World,
};
use smallvec::SmallVec;

const THRESHOLD: f32 = 0.0001;
const THRESHOLD2: f32 = THRESHOLD * THRESHOLD;
const SPEED: f32 = 0.1;

#[derive(Clone, Copy, Debug, Component)]
struct Pos {
    x: f32,
    y: f32,
}

impl Pos {
    fn distance(&self, other: Pos) -> f32 {
        self.distance2(other).sqrt()
    }

    fn distance2(&self, other: Pos) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        dx * dx + dy * dy
    }

    fn dir_to(&self, other: Pos) -> (f32, f32) {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        let distance = self.distance(other);
        (dx / distance, dy / distance)
    }
}

#[derive(Debug, Component)]
struct MoveTo {
    target: Pos,
    speed: f32,
    wakers: SmallVec<[Waker; 2]>,
}

struct DeltaTime(f32);

fn move_to_system(
    dt: Res<DeltaTime>,
    view: View<(Entities, &mut Pos, &mut MoveTo)>,
    mut encoder: ActionEncoder,
) {
    let dt = dt.0;

    for (entity, pos, move_to) in view {
        let step = dt * move_to.speed;

        let distance2 = pos.distance2(move_to.target);
        if distance2 <= step {
            pos.x = move_to.target.x;
            pos.y = move_to.target.y;

            for waker in move_to.wakers.drain(..) {
                waker.wake();
            }
            encoder.drop::<MoveTo>(entity);
        } else {
            let dir = pos.dir_to(move_to.target);
            pos.x += dir.0 * step;
            pos.y += dir.1 * step;
        }
    }
}

async fn move_to(e: &mut Entity<'_>, target: Pos) {
    e.poll_ref(move |mut e, cx| {
        let Some(pos) = e.get::<&Pos>() else {
            return Poll::Ready(());
        };

        if pos.distance2(target) < THRESHOLD2 {
            return Poll::Ready(());
        }

        match e.get_mut::<&mut MoveTo>() {
            None => {
                e.insert(MoveTo {
                    target,
                    speed: SPEED,
                    wakers: smallvec::smallvec![cx.waker().clone()],
                });
                Poll::Pending
            }
            Some(move_to) => {
                move_to.target = target;
                move_to.speed = SPEED;

                for waker in move_to.wakers.iter() {
                    if waker.will_wake(cx.waker()) {
                        return Poll::Pending;
                    }
                }

                move_to.wakers.push(cx.waker().clone());
                Poll::Pending
            }
        }
    })
    .await;
}

#[derive(Component)]
struct Finish;

fn main() {
    let mut flows = Flows::new();
    let mut world = World::new();

    let e = world.spawn((Pos { x: 0.0, y: 1.0 },)).id();

    let targets = [
        Pos { x: 1.0, y: 0.0 },
        Pos { x: 0.0, y: -1.0 },
        Pos { x: -1.0, y: 0.0 },
    ];

    let mut scheduler = Scheduler::new();
    scheduler.add_system(move_to_system);

    world.spawn_flow_for(
        e,
        flow_fn!(|mut e| {
            for target in targets {
                move_to(&mut e, target).await;
            }
            let _ = e.insert(Finish);
        }),
    );

    loop {
        world.insert_resource(DeltaTime(0.1));

        flows.execute(&mut world);
        scheduler.run_sequential(&mut world);

        if world.try_has_component::<Finish>(e).unwrap() {
            return;
        }

        println!("{:?}", world.get::<&Pos>(e).unwrap());
    }
}
