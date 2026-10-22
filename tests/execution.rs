use edict::{
    action::{ActionBuffer, LocalActionBuffer},
    component::Component,
    resources::{Res, ResMut},
    world::World,
};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

#[derive(Component, Debug, PartialEq, Eq)]
struct Value(i32);

#[test]
fn action_buffer_reserves_ids_and_applies_ordered_mutations() {
    let mut world = World::new();
    let mut buffer = ActionBuffer::new();
    let id;
    {
        let mut encoder = buffer.encoder(&world);
        id = encoder.spawn_one(Value(1)).id();
        encoder.insert(id, Value(2));
        encoder.with(id, || Value(99));
    }
    assert!(world.is_alive(id));
    assert!(world.view::<&Value>().iter().next().is_none());
    assert!(buffer.execute(&mut world));
    assert_eq!(world.get::<&Value>(id).unwrap().0, 2);
    assert!(!buffer.execute(&mut world));
}

#[test]
fn deferred_closures_observe_prior_actions() {
    let mut world = World::new();
    let mut buffer = ActionBuffer::new();
    {
        let mut encoder = buffer.encoder(&world);
        encoder.insert_resource(vec![1_i32]);
        encoder.closure(|world| world.expect_resource_mut::<Vec<i32>>().push(2));
        encoder.closure(|world| assert_eq!(*world.expect_resource::<Vec<i32>>(), vec![1, 2]));
    }
    assert!(world.get_resource::<Vec<i32>>().is_none());
    buffer.execute(&mut world);
    assert_eq!(world.clone_resource::<Vec<i32>>(), vec![1, 2]);
}

#[test]
fn deferred_actions_safely_skip_despawned_entities() {
    let mut world = World::new();
    let id = world.spawn_one(Value(1)).id();
    let mut buffer = ActionBuffer::new();
    {
        let mut encoder = buffer.encoder(&world);
        encoder.despawn(id);
        encoder.insert(id, Value(2));
        encoder.drop::<Value>(id);
        encoder.despawn(id);
    }
    buffer.execute(&mut world);
    assert!(!world.is_alive(id));
}

#[test]
fn local_action_buffer_accepts_non_send_captures() {
    let mut world = World::new();
    let calls = Rc::new(Cell::new(0));
    let copy = calls.clone();
    let mut buffer = LocalActionBuffer::new();
    buffer.encoder(&world).closure(move |world| {
        copy.set(copy.get() + 1);
        world.insert_resource(copy);
    });
    assert_eq!(calls.get(), 0);
    assert!(buffer.execute(&mut world));
    assert_eq!(calls.get(), 1);
    assert!(Rc::ptr_eq(
        &calls,
        &world.local().expect_resource::<Rc<Cell<i32>>>()
    ));
}

#[test]
fn action_sender_thread_preserves_submission_order() {
    let mut world = World::new();
    world.insert_resource(Vec::<usize>::new());
    let sender = world.new_action_sender();
    std::thread::spawn(move || {
        for i in 0..64 {
            sender.closure(move |world| world.expect_resource_mut::<Vec<usize>>().push(i));
        }
    })
    .join()
    .unwrap();
    assert!(world.expect_resource::<Vec<usize>>().is_empty());
    world.execute_received_actions();
    assert_eq!(
        *world.expect_resource::<Vec<usize>>(),
        (0..64).collect::<Vec<_>>()
    );
}

#[test]
fn cloned_action_senders_deliver_all_threaded_commands() {
    let mut world = World::new();
    world.insert_resource(0_usize);
    let sender = world.new_action_sender();
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let sender = sender.clone();
            std::thread::spawn(move || {
                for _ in 0..32 {
                    sender.closure(|world| *world.expect_resource_mut::<usize>() += 1);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    world.execute_received_actions();
    assert_eq!(world.copy_resource::<usize>(), 128);
    assert!(sender.is_connected());
    drop(world);
    assert!(!sender.is_connected());
}

#[test]
fn resources_with_remove_and_type_iteration() {
    let mut world = World::new();
    *world.with_resource(|| String::from("hello")) += " world";
    world.with_resource::<String>(|| panic!("existing resource initializer must stay lazy"));
    *world.with_default_resource::<u32>() += 7;
    let mut types = world.resource_types().collect::<Vec<_>>();
    types.sort();
    let mut expected = vec![
        std::any::TypeId::of::<String>(),
        std::any::TypeId::of::<u32>(),
    ];
    expected.sort();
    assert_eq!(types, expected);
    assert_eq!(
        world.remove_resource::<String>(),
        Some("hello world".into())
    );
    assert_eq!(world.remove_resource::<String>(), None);
    assert_eq!(world.copy_resource::<u32>(), 7);
}

#[test]
fn resource_shared_borrows_exclude_writer_until_guards_drop() {
    let mut world = World::new();
    world.insert_resource(10_i32);
    let a = world.expect_resource::<i32>();
    let b = world.expect_resource::<i32>();
    assert_eq!(*a + *b, 20);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _guard = world.expect_resource_mut::<i32>();
        }))
        .is_err()
    );
    drop((a, b));
    *world.expect_resource_mut::<i32>() = 20;
    assert_eq!(world.copy_resource::<i32>(), 20);
}

#[test]
fn resource_writer_excludes_reader_and_second_writer() {
    let mut world = World::new();
    world.insert_resource(10_i32);
    let mut writer = world.expect_resource_mut::<i32>();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _guard = world.expect_resource::<i32>();
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _guard = world.expect_resource_mut::<i32>();
        }))
        .is_err()
    );
    *writer = 11;
    drop(writer);
    assert_eq!(world.copy_resource::<i32>(), 11);
}

#[test]
fn resource_leaks_can_be_reset_after_last_use() {
    let mut world = World::new();
    world.insert_resource(1_i32);
    *ResMut::leak(world.expect_resource_mut::<i32>()) = 9;
    world.undo_resource_leaks();
    assert_eq!(*Res::leak(world.expect_resource::<i32>()), 9);
    world.undo_resource_leaks();
    *world.expect_resource_mut::<i32>() += 1;
    assert_eq!(world.copy_resource::<i32>(), 10);
}

#[cfg(feature = "scheduler")]
mod schedules {
    use super::*;
    use edict::{scheduler::Scheduler, system::State, view::View};

    fn exercise(run: fn(&mut Scheduler, &mut World)) {
        let mut world = World::new();
        world.insert_resource(Vec::<i32>::new());
        let first = world.spawn_one(Value(0)).id();
        let mut schedule = Scheduler::new();
        schedule.add_system(|mut state: State<i32>, view: View<&mut Value>| {
            *state += 1;
            for value in view {
                value.0 += *state;
            }
        });
        schedule.add_system(|view: View<&Value>, mut observed: ResMut<Vec<i32>>| {
            let mut sum = 0;
            for value in view {
                sum += value.0;
            }
            observed.push(sum);
        });
        run(&mut schedule, &mut world);
        run(&mut schedule, &mut world);
        assert_eq!(world.get::<&Value>(first).unwrap().0, 3);
        #[derive(Component)]
        struct NewArchetype;
        let second = world.spawn((Value(10), NewArchetype)).id();
        run(&mut schedule, &mut world);
        assert_eq!(world.get::<&Value>(first).unwrap().0, 6);
        assert_eq!(world.get::<&Value>(second).unwrap().0, 13);
        assert_eq!(*world.expect_resource::<Vec<i32>>(), vec![1, 3, 19]);
    }
    #[test]
    fn sequential_schedule_preserves_state_conflicts_and_rebuilds_for_archetypes() {
        exercise(Scheduler::run_sequential);
    }
    #[cfg(feature = "threaded-scheduler")]
    #[test]
    fn threaded_schedule_preserves_state_conflicts_and_rebuilds_for_archetypes() {
        exercise(Scheduler::run_threaded);
    }
    #[cfg(feature = "rayon-scheduler")]
    #[test]
    fn rayon_schedule_preserves_state_conflicts_and_rebuilds_for_archetypes() {
        exercise(Scheduler::run_rayon);
    }

    #[test]
    fn state_is_private_to_each_system() {
        let mut world = World::new();
        world.insert_resource(Vec::<usize>::new());
        let mut schedule = Scheduler::new();
        fn record(mut state: State<usize>, mut log: ResMut<Vec<usize>>) {
            *state += 1;
            log.push(*state);
        }
        schedule.add_system(record);
        schedule.add_system(record);
        schedule.run_sequential(&mut world);
        schedule.run_sequential(&mut world);
        assert_eq!(*world.expect_resource::<Vec<usize>>(), vec![1, 1, 2, 2]);
    }
}

#[cfg(all(feature = "flow", feature = "std"))]
mod flows {
    use super::*;
    use edict::flow::{FlowEntity, FlowWorld, Flows};
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::Poll,
    };

    #[test]
    fn flow_yields_once_and_completes_once() {
        let mut world = World::new();
        let mut flows = Flows::new();
        world.spawn_flow(|world: FlowWorld| async move {
            world.insert_resource(1_i32);
            edict::yield_now!();
            world.insert_resource(2_i32);
        });
        assert!(world.get_resource::<i32>().is_none());
        flows.execute(&mut world);
        assert_eq!(world.copy_resource::<i32>(), 1);
        flows.execute(&mut world);
        assert_eq!(world.copy_resource::<i32>(), 2);
        world.insert_resource(3_i32);
        flows.execute(&mut world);
        assert_eq!(world.copy_resource::<i32>(), 3);
    }

    #[test]
    fn flow_external_waker_controls_repolling() {
        let mut world = World::new();
        let mut flows = Flows::new();
        let polls = Arc::new(AtomicUsize::new(0));
        let waker = Arc::new(Mutex::new(None));
        let task_polls = polls.clone();
        let task_waker = waker.clone();
        world.spawn_flow(move |world: FlowWorld| async move {
            world
                .poll(move |_, cx| {
                    let poll = task_polls.fetch_add(1, Ordering::SeqCst);
                    if poll == 0 {
                        *task_waker.lock().unwrap() = Some(cx.waker().clone());
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                })
                .await;
            world.insert_resource(true);
        });
        flows.execute(&mut world);
        flows.execute(&mut world);
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        waker.lock().unwrap().take().unwrap().wake();
        flows.execute(&mut world);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(world.copy_resource::<bool>());
    }

    #[test]
    fn entity_flow_cancels_and_drops_future_on_despawn() {
        struct DropCount(Arc<AtomicUsize>);
        impl Drop for DropCount {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let mut world = World::new();
        let mut flows = Flows::new();
        let entity = world.spawn(()).id();
        let drops = Arc::new(AtomicUsize::new(0));
        let guard = DropCount(drops.clone());
        world.spawn_flow_for(entity, move |entity: FlowEntity| async move {
            let _guard = guard;
            entity.poll(|_, _| Poll::<()>::Pending).await;
            panic!("despawned entity flow must cancel");
        });
        flows.execute(&mut world);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        world.despawn(entity).unwrap();
        flows.execute(&mut world);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn flow_waits_for_component_inserted_between_ticks() {
        let mut world = World::new();
        let mut flows = Flows::new();
        let entity = world.spawn(()).id();
        world.spawn_flow_for(entity, |entity: FlowEntity| async move {
            entity.wait_has_component::<Value>().await;
            entity.insert(Value(42));
        });
        flows.execute(&mut world);
        assert!(!world.try_has_component::<Value>(entity).unwrap());
        world.insert(entity, Value(1)).unwrap();
        flows.execute(&mut world);
        assert_eq!(world.get::<&Value>(entity).unwrap().0, 42);
    }

    #[test]
    fn world_flow_observes_entity_despawn() {
        let mut world = World::new();
        let mut flows = Flows::new();
        let entity = world.spawn(()).id();
        world.spawn_flow(move |world: FlowWorld| async move {
            world.entity(entity).unwrap().wait_despawned().await;
            world.insert_resource(true);
        });
        flows.execute(&mut world);
        assert!(world.get_resource::<bool>().is_none());
        world.despawn(entity).unwrap();
        flows.execute(&mut world);
        assert!(world.copy_resource::<bool>());
    }
}

#[test]
fn world_local_deferred_resources_execute_in_order() {
    let mut world = World::new();
    let local = world.local();
    local.insert_resource_defer(5_i32);
    local.defer(|world| *world.expect_resource_mut::<i32>() += 2);
    local.drop_resource_defer::<i32>();
    local.insert_resource_defer(11_i32);
    assert!(local.get_resource::<i32>().is_none());
    local.run_deferred();
    assert_eq!(world.copy_resource::<i32>(), 11);
}
