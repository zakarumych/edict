#![cfg(feature = "flow")]

use edict::{
    component::Component,
    flow::{FlowWorld, Flows},
    world::World,
};
use std::{
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    task::{Context, Poll, Waker},
};

#[cfg(not(feature = "std"))]
mod host {
    use std::{cell::Cell, ptr::NonNull};
    std::thread_local! {
        static WORLD: Cell<Option<NonNull<u8>>> = const { Cell::new(None) };
        static BORROWED: Cell<bool> = const { Cell::new(false) };
    }
    #[unsafe(no_mangle)]
    extern "C" fn edict_set_flow_world_tls(world: NonNull<u8>) -> Option<NonNull<u8>> {
        WORLD.with(|slot| slot.replace(Some(world)))
    }
    #[unsafe(no_mangle)]
    extern "C" fn edict_get_flow_world_tls() -> Option<NonNull<u8>> {
        WORLD.with(Cell::get)
    }
    #[unsafe(no_mangle)]
    extern "C" fn edict_reset_flow_world_tls(prev: Option<NonNull<u8>>, world: NonNull<u8>) {
        WORLD.with(|slot| assert_eq!(slot.replace(prev), Some(world)));
    }
    #[unsafe(no_mangle)]
    extern "C" fn edict_replace_flow_world_borrowed_tls(value: bool) -> bool {
        BORROWED.with(|slot| slot.replace(value))
    }
}

#[derive(Component)]
struct Count(u32);

fn rejects(f: impl FnOnce()) {
    assert!(catch_unwind(AssertUnwindSafe(f)).is_err());
}

#[test]
fn callbacks_reject_reentry_and_recover() {
    let mut world = World::new();
    Flows::enter(&mut world, |flow| {
        let entity = flow.spawn((Count(1),));
        rejects(|| flow.map(|_| flow.map(|_| ())));
        rejects(|| {
            flow.map(|_| {
                entity.is_alive();
            })
        });
        rejects(|| {
            entity.map(|_| {
                flow.spawn((Count(2),));
            })
        });
        rejects(|| {
            flow.map(|local| {
                let view = local.view::<&Count>();
                assert_eq!(view.iter().count(), 1);
                flow.spawn((Count(3),));
            })
        });
        rejects(|| flow.map(|_| panic!("callback failure")));
        assert!(entity.is_alive());
        assert_eq!(
            flow.map(|local| local.view::<&Count>().iter().map(|c| c.0).sum::<u32>()),
            1
        );
    });
}

#[derive(Component)]
struct ReentrantClone(FlowWorld);
impl Clone for ReentrantClone {
    fn clone(&self) -> Self {
        self.0.map(|_| ());
        Self(self.0)
    }
}

#[test]
fn clones_keep_guard_held() {
    let mut world = World::new();
    Flows::enter(&mut world, |flow| {
        let entity = flow.spawn((ReentrantClone(flow),));
        flow.insert_resource(ReentrantClone(flow));
        rejects(|| {
            let _ = flow.try_get_cloned::<ReentrantClone>(entity);
        });
        rejects(|| {
            let _ = flow.clone_resource::<ReentrantClone>();
        });
        assert!(entity.is_alive());
    });
}

#[test]
fn polled_callbacks_and_nested_contexts_are_checked() {
    let mut world = World::new();
    Flows::enter(&mut world, |flow| {
        let entity = flow.spawn((Count(1),));
        let mut cx = Context::from_waker(Waker::noop());
        rejects(|| {
            flow.map(|_| {
                let mut future = core::pin::pin!(flow.poll(|_, _| Poll::Ready(())));
                let _ = future.as_mut().poll(&mut cx);
            })
        });
        let mut future = core::pin::pin!(flow.poll_view::<&Count, _, ()>(|_, _| {
            flow.map(|_| ());
            Poll::Ready(())
        }));
        rejects(|| {
            let _ = future.as_mut().poll(&mut cx);
        });
        rejects(|| flow.map(|local| Flows::enter(local.as_world(), |_| ())));
        assert!(entity.is_alive());
    });
    let mut other = World::new();
    Flows::enter(&mut world, |outer| {
        Flows::enter(&mut other, |inner| {
            inner.spawn((Count(2),));
        });
        outer.spawn((Count(3),));
    });
    assert_eq!(world.view::<&Count>().iter().count(), 2);
    assert_eq!(other.view::<&Count>().iter().count(), 1);
}

#[test]
fn owned_batch_callbacks_can_access_world() {
    let mut world = World::new();
    Flows::enter(&mut world, |flow| {
        let count = flow
            .spawn_batch((0..4).map(|n| {
                flow.map(|_| ());
                (Count(n),)
            }))
            .fold(0, |count, entity| {
                assert!(entity.is_alive());
                flow.map(|_| ());
                count + 1
            });
        assert_eq!(count, 4);
        assert_eq!(
            flow.spawn_batch((4..7).map(|n| (Count(n),)))
                .rfold(0, |count, entity| {
                    assert!(entity.is_alive());
                    count + 1
                }),
            3
        );
    });
    assert_eq!(world.view::<&Count>().iter().count(), 7);
}

#[test]
fn copied_handle_cannot_access_other_threads_context() {
    let mut world = World::new();
    Flows::enter(&mut world, |flow| {
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    rejects(|| flow.map(|_| ()));
                    rejects(|| flow.map(|_| ()));
                    let mut local = World::new();
                    Flows::enter(&mut local, |bound| {
                        bound.spawn((Count(5),));
                    });
                    assert_eq!(local.view::<&Count>().iter().count(), 1);
                })
                .join()
                .unwrap();
        });
        flow.spawn((Count(6),));
    });
    assert_eq!(world.view::<&Count>().iter().count(), 1);
}
