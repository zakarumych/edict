#![cfg(feature = "scheduler")]

use edict::{
    action::{ActionEncoder, LocalActionEncoder},
    component::Component,
    entity::EntityId,
    query::{Modified, With},
    relation::{ChildOf, Relates},
    resources::{Res, ResMut},
    scheduler::Scheduler,
    system::State,
    view::View,
    world::World,
};

#[derive(Component)]
struct Label(u32);
#[derive(Component)]
struct Position(i32);
#[derive(Component)]
struct Promoted;
#[derive(Component)]
struct Spawned;
struct Receipt(u32);
impl Component for Receipt {
    fn on_drop(&mut self, _: EntityId, mut encoder: LocalActionEncoder) {
        let label = self.0;
        encoder.closure(move |world| world.expect_resource_mut::<Dropped>().0.push(label));
    }
}
#[derive(Default)]
struct Dropped(Vec<u32>);
#[derive(Default)]
struct Tick(u32);
struct Family {
    parent: EntityId,
    first: EntityId,
}
#[derive(Default)]
struct Reserved(Option<EntityId>);
#[derive(Default)]
struct BeforeActions(Vec<Vec<(u32, i32)>>);
#[derive(Default)]
struct Changes(Vec<Vec<(u32, i32)>>);
#[derive(Default)]
struct LinkCounts(Vec<usize>);

type Runner = fn(&mut Scheduler, &mut World);

fn advance(mut state: State<u32>, mut tick: ResMut<Tick>) {
    *state += 1;
    tick.0 = *state;
}
fn movement(tick: Res<Tick>, view: View<&mut Position>) {
    if tick.0 == 3 {
        return;
    }
    for position in view {
        position.0 += tick.0 as i32;
    }
}
fn changed_positions(view: View<(&Label, Modified<&Position>)>, mut changes: ResMut<Changes>) {
    let mut values = view
        .into_iter()
        .map(|(label, position)| (label.0, position.0))
        .collect::<Vec<_>>();
    values.sort();
    changes.0.push(values);
}
fn family_commands(
    tick: Res<Tick>,
    family: Res<Family>,
    mut reserved: ResMut<Reserved>,
    mut encoder: ActionEncoder,
) {
    match tick.0 {
        1 => {
            let child = encoder
                .spawn((Label(1), Position(100), Receipt(1), Spawned))
                .id();
            encoder.insert(child, Promoted);
            encoder.insert_relation(child, ChildOf, family.parent);
            reserved.0 = Some(child);
        }
        2 => encoder.insert(family.first, Promoted),
        4 => encoder.despawn(family.parent),
        _ => {}
    }
}
fn observe_before_actions(
    view: View<(&Label, &Position)>,
    reserved: Res<Reserved>,
    mut snapshots: ResMut<BeforeActions>,
) {
    let mut values = view
        .into_iter()
        .map(|(label, position)| (label.0, position.0))
        .collect::<Vec<_>>();
    values.sort();
    if snapshots.0.is_empty() {
        assert!(
            reserved.0.is_some(),
            "producer must reserve child before observer"
        );
        assert_eq!(values.len(), 1, "reserved child components stay deferred");
    }
    snapshots.0.push(values);
}
fn observe_links(view: View<Relates<With<ChildOf>>>, mut counts: ResMut<LinkCounts>) {
    counts
        .0
        .push(view.into_iter().map(|targets| targets.count()).sum());
}
fn position_snapshot(world: &World) -> Vec<(u32, i32)> {
    let mut values = world
        .view::<(&Label, &Position)>()
        .iter()
        .map(|(label, position)| (label.0, position.0))
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn replay_family(run: Runner) {
    let mut world = World::new();
    let parent = world.spawn(()).id();
    let first = world.spawn((Label(0), Position(10), Receipt(0))).id();
    world.insert_relation(first, ChildOf, parent).unwrap();
    world.insert_resource(Family { parent, first });
    world.insert_resource(Tick::default());
    world.insert_resource(Reserved::default());
    world.insert_resource(BeforeActions::default());
    world.insert_resource(Changes::default());
    world.insert_resource(LinkCounts::default());
    world.insert_resource(Dropped::default());
    let mut schedule = Scheduler::new();
    schedule.add_system(advance);
    schedule.add_system(movement);
    schedule.add_system(changed_positions);
    schedule.add_system(family_commands);
    schedule.add_system(observe_before_actions);
    schedule.add_system(observe_links);

    let before_growth = world.archetype_set_id();
    run(&mut schedule, &mut world);
    let second = world.expect_resource::<Reserved>().0.unwrap();
    assert_ne!(world.archetype_set_id(), before_growth);
    assert_eq!(position_snapshot(&world), vec![(0, 11), (1, 100)]);
    assert!(world.try_has_component::<Promoted>(second).unwrap());
    assert_eq!(
        world
            .get::<Relates<With<ChildOf>>>(second)
            .unwrap()
            .map(|e| e.id())
            .collect::<Vec<_>>(),
        vec![parent]
    );
    assert_eq!(
        world.expect_resource::<BeforeActions>().0,
        vec![vec![(0, 11)]]
    );
    assert_eq!(world.expect_resource::<LinkCounts>().0, vec![1]);

    let before_promotion = world.archetype_set_id();
    run(&mut schedule, &mut world);
    assert_ne!(world.archetype_set_id(), before_promotion);
    assert!(world.try_has_component::<Promoted>(first).unwrap());
    assert_eq!(position_snapshot(&world), vec![(0, 13), (1, 102)]);
    run(&mut schedule, &mut world);
    assert_eq!(position_snapshot(&world), vec![(0, 13), (1, 102)]);
    assert!(
        world.expect_resource::<Changes>().0[2].is_empty(),
        "archetype migration must not mark unchanged positions"
    );
    run(&mut schedule, &mut world);
    assert!(position_snapshot(&world).is_empty());
    for id in [parent, first, second] {
        assert!(!world.is_alive(id));
    }
    let mut dropped = world.expect_resource::<Dropped>().0.clone();
    dropped.sort();
    assert_eq!(dropped, vec![0, 1]);
    run(&mut schedule, &mut world);
    assert_eq!(
        world.expect_resource::<BeforeActions>().0,
        vec![
            vec![(0, 11)],
            vec![(0, 13), (1, 102)],
            vec![(0, 13), (1, 102)],
            vec![(0, 17), (1, 106)],
            vec![]
        ]
    );
    assert_eq!(
        world.expect_resource::<Changes>().0,
        vec![
            vec![(0, 11)],
            vec![(0, 13), (1, 102)],
            vec![],
            vec![(0, 17), (1, 106)],
            vec![]
        ]
    );
    assert_eq!(world.expect_resource::<LinkCounts>().0, vec![1, 2, 2, 2, 0]);
    assert_eq!(world.expect_resource::<Tick>().0, 5);
}

#[test]
fn sequential_family_lifecycle_replays_component_and_relation_transactions() {
    replay_family(Scheduler::run_sequential);
}
#[cfg(feature = "threaded-scheduler")]
#[test]
fn threaded_family_lifecycle_replays_component_and_relation_transactions() {
    replay_family(Scheduler::run_threaded);
}
#[cfg(feature = "rayon-scheduler")]
#[test]
fn rayon_family_lifecycle_replays_component_and_relation_transactions() {
    replay_family(Scheduler::run_rayon);
}

#[cfg(all(feature = "flow", feature = "std"))]
mod async_jobs {
    use super::*;
    use edict::flow::{FlowEntity, Flows};
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        task::Poll,
    };

    #[derive(Component)]
    struct Ready;
    #[derive(Component)]
    struct Work(i32);
    struct Rate(i32);
    struct JobControl {
        parent: EntityId,
        child: EntityId,
        activate: bool,
    }
    #[derive(Debug, PartialEq, Eq)]
    struct Frame {
        tick: u32,
        ready: usize,
        work: Vec<i32>,
        links: usize,
    }
    #[derive(Default)]
    struct Frames(Vec<Frame>);
    #[derive(Default)]
    struct Totals(Vec<i32>);
    struct DropFuture(Arc<AtomicUsize>);
    impl Drop for DropFuture {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn activate_jobs(tick: Res<Tick>, control: Res<JobControl>, mut actions: ActionEncoder) {
        if tick.0 == 1 && control.activate {
            actions.insert(control.child, Ready);
        }
    }
    fn process_jobs(view: View<&mut Work>, rate: Res<Rate>) {
        for work in view {
            work.0 += rate.0;
        }
    }
    fn total_jobs(view: View<&Work>, mut totals: ResMut<Totals>) {
        totals.0.push(view.into_iter().map(|work| work.0).sum());
    }
    fn capture_jobs(
        tick: Res<Tick>,
        ready: View<&Ready>,
        work: View<&Work>,
        links: View<Relates<With<ChildOf>>>,
        mut frames: ResMut<Frames>,
    ) {
        let mut values = work.into_iter().map(|work| work.0).collect::<Vec<_>>();
        values.sort();
        frames.0.push(Frame {
            tick: tick.0,
            ready: ready.into_iter().count(),
            work: values,
            links: links.into_iter().map(|targets| targets.count()).sum(),
        });
    }
    fn retire(tick: Res<Tick>, control: Res<JobControl>, mut actions: ActionEncoder) {
        if tick.0 == 3 {
            actions.despawn(control.parent);
        }
    }
    fn setup(activate: bool, linked: bool) -> (World, Scheduler, EntityId, EntityId) {
        let mut world = World::new();
        let parent = world.spawn(()).id();
        let child = world.spawn_one(Receipt(7)).id();
        if linked {
            world.insert_relation(child, ChildOf, parent).unwrap();
        }
        world.insert_resource(JobControl {
            parent,
            child,
            activate,
        });
        world.insert_resource(Tick::default());
        world.insert_resource(Rate(1));
        world.insert_resource(Frames::default());
        world.insert_resource(Totals::default());
        world.insert_resource(Dropped::default());
        let mut schedule = Scheduler::new();
        schedule.add_system(advance);
        schedule.add_system(activate_jobs);
        schedule.add_system(process_jobs);
        schedule.add_system(total_jobs);
        schedule.add_system(capture_jobs);
        schedule.add_system(retire);
        (world, schedule, parent, child)
    }

    #[test]
    fn component_wait_drives_scheduled_work_until_parent_cascade_cancels_flow() {
        let (mut world, mut schedule, parent, child) = setup(true, true);
        let mut flows = Flows::new();
        let cancelled = Arc::new(AtomicUsize::new(0));
        let guard = DropFuture(cancelled.clone());
        world.spawn_flow_for(child, move |entity: FlowEntity| async move {
            let _guard = guard;
            entity.wait_has_component::<Ready>().await;
            entity.insert(Work(5));
            entity.poll(|_, _| Poll::<()>::Pending).await;
            panic!("owned entity flow must cancel when parent despawns");
        });
        flows.execute(&mut world);
        assert_eq!(cancelled.load(Ordering::SeqCst), 0);
        assert!(!world.try_has_component::<Ready>(child).unwrap());
        schedule.run_sequential(&mut world);
        assert!(world.try_has_component::<Ready>(child).unwrap());
        assert!(!world.try_has_component::<Work>(child).unwrap());
        assert_eq!(
            world.expect_resource::<Frames>().0,
            vec![Frame {
                tick: 1,
                ready: 0,
                work: vec![],
                links: 1
            }]
        );
        flows.execute(&mut world);
        assert_eq!(world.get::<&Work>(child).unwrap().0, 5);
        schedule.run_sequential(&mut world);
        assert_eq!(world.get::<&Work>(child).unwrap().0, 6);
        flows.execute(&mut world);
        assert_eq!(cancelled.load(Ordering::SeqCst), 0);
        schedule.run_sequential(&mut world);
        assert!(!world.is_alive(parent));
        assert!(!world.is_alive(child));
        assert_eq!(world.expect_resource::<Dropped>().0, vec![7]);
        assert_eq!(
            cancelled.load(Ordering::SeqCst),
            0,
            "executor owns pending future until cancellation is polled"
        );
        flows.execute(&mut world);
        assert_eq!(cancelled.load(Ordering::SeqCst), 1);
        schedule.run_sequential(&mut world);
        flows.execute(&mut world);
        assert_eq!(world.expect_resource::<Totals>().0, vec![0, 6, 7, 0]);
        assert_eq!(
            world.expect_resource::<Frames>().0,
            vec![
                Frame {
                    tick: 1,
                    ready: 0,
                    work: vec![],
                    links: 1
                },
                Frame {
                    tick: 2,
                    ready: 1,
                    work: vec![6],
                    links: 1
                },
                Frame {
                    tick: 3,
                    ready: 1,
                    work: vec![7],
                    links: 1
                },
                Frame {
                    tick: 4,
                    ready: 0,
                    work: vec![],
                    links: 0
                }
            ]
        );
        assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn sender_ingress_and_explicit_wakeup_publish_work_before_scheduled_cascade() {
        let (mut world, mut schedule, parent, child) = setup(false, false);
        let mut flows = Flows::new();
        let cancelled = Arc::new(AtomicUsize::new(0));
        let guard = DropFuture(cancelled.clone());
        let allowed = Arc::new(AtomicBool::new(false));
        let task_allowed = allowed.clone();
        let saved_waker = Arc::new(Mutex::new(None));
        let task_waker = saved_waker.clone();
        let polls = Arc::new(AtomicUsize::new(0));
        let task_polls = polls.clone();
        world.spawn_flow_for(child, move |entity: FlowEntity| async move {
            let _guard = guard;
            entity.wait_has_component::<Ready>().await;
            entity
                .poll(move |_, cx| {
                    task_polls.fetch_add(1, Ordering::SeqCst);
                    if task_allowed.load(Ordering::SeqCst) {
                        Poll::Ready(())
                    } else {
                        *task_waker.lock().unwrap() = Some(cx.waker().clone());
                        Poll::Pending
                    }
                })
                .await;
            entity
                .world()
                .insert_relation(entity.id(), ChildOf, parent)
                .unwrap();
            entity.insert(Work(40));
            entity.poll(|_, _| Poll::<()>::Pending).await;
            panic!("owned entity flow must cancel");
        });
        flows.execute(&mut world);
        let sender = world.new_action_sender();
        let initial_sender = sender.clone();
        std::thread::spawn(move || {
            initial_sender.insert(child, Ready);
            initial_sender.insert_resource(Rate(3));
        })
        .join()
        .unwrap();
        assert!(!world.try_has_component::<Ready>(child).unwrap());
        assert_eq!(world.expect_resource::<Rate>().0, 1);
        world.execute_received_actions();
        assert!(world.try_has_component::<Ready>(child).unwrap());
        assert_eq!(world.expect_resource::<Rate>().0, 3);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        flows.execute(&mut world);
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        schedule.run_sequential(&mut world);
        flows.execute(&mut world);
        assert_eq!(
            polls.load(Ordering::SeqCst),
            1,
            "pending flow must await explicit wake"
        );
        assert_eq!(
            world.expect_resource::<Frames>().0,
            vec![Frame {
                tick: 1,
                ready: 1,
                work: vec![],
                links: 0
            }]
        );
        std::thread::spawn(move || {
            sender.closure(move |_| {
                allowed.store(true, Ordering::SeqCst);
            })
        })
        .join()
        .unwrap();
        world.execute_received_actions();
        flows.execute(&mut world);
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        saved_waker.lock().unwrap().take().unwrap().wake();
        assert!(!world.try_has_component::<Work>(child).unwrap());
        flows.execute(&mut world);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert_eq!(world.get::<&Work>(child).unwrap().0, 40);
        assert_eq!(
            world
                .get::<Relates<With<ChildOf>>>(child)
                .unwrap()
                .map(|e| e.id())
                .collect::<Vec<_>>(),
            vec![parent]
        );
        schedule.run_sequential(&mut world);
        assert_eq!(world.get::<&Work>(child).unwrap().0, 43);
        schedule.run_sequential(&mut world);
        assert!(!world.is_alive(child));
        assert!(!world.is_alive(parent));
        assert_eq!(world.expect_resource::<Dropped>().0, vec![7]);
        assert_eq!(cancelled.load(Ordering::SeqCst), 0);
        flows.execute(&mut world);
        assert_eq!(cancelled.load(Ordering::SeqCst), 1);
        schedule.run_sequential(&mut world);
        assert_eq!(world.expect_resource::<Totals>().0, vec![0, 43, 46, 0]);
        assert_eq!(
            world.expect_resource::<Frames>().0,
            vec![
                Frame {
                    tick: 1,
                    ready: 1,
                    work: vec![],
                    links: 0
                },
                Frame {
                    tick: 2,
                    ready: 1,
                    work: vec![43],
                    links: 1
                },
                Frame {
                    tick: 3,
                    ready: 1,
                    work: vec![46],
                    links: 1
                },
                Frame {
                    tick: 4,
                    ready: 0,
                    work: vec![],
                    links: 0
                }
            ]
        );
    }
}
