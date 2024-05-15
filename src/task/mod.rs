//! This module provides mechanism for storing async tasks in ECS, potentially attaching them to existing entities.
//! Tasks are executed by [`task_system`].

use std::{
    any::TypeId,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    ptr::NonNull,
    task::{Context, Poll, Waker},
};

use crate::{
    archetype::{first_of_chunk, CHUNK_LEN},
    component::{Component, ComponentBorrow},
    entity::EntityId,
    epoch::EpochId,
    system::State,
    world::World,
};

#[derive(Clone)]
struct TaskWaker<T> {
    id: EntityId,
    task: PhantomData<fn() -> T>,
    queue: Arc<Mutex<Vec<(EntityId, TypeId)>>>,
}

impl<T: 'static> Wake for TaskWaker<T> {
    #[inline(always)]
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    #[inline(always)]
    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.lock().push((self.id, type_id::<T>()));
    }
}

impl<T: 'static> TaskWaker<T> {
    fn waker(id: EntityId, queue: Arc<Mutex<Vec<(EntityId, TypeId)>>>) -> Waker {
        let waker = Self {
            id,
            task: PhantomData,
            queue,
        };
        Waker::from(Arc::new(waker))
    }
}

/// Component that stores async task.
/// Marker type is used to distinguish between different tasks kinds
/// even if future type is the same.
pub struct Task<T = (), F = Pin<Box<dyn Future<Output = ()> + Send>>> {
    fut: F,
    marker: PhantomData<fn() -> T>,
}

impl<T, F> Task<T, F> {
    /// Creates new task wrapping a `Unpin` future.
    /// Inserting this as component will
    /// cause [`task_system`] to poll it until completion.
    pub fn new(fut: F) -> Self
    where
        F: Future<Output = ()> + Unpin + Send + 'static,
    {
        Self {
            fut,
            marker: PhantomData,
        }
    }
}

impl<T> Task<T> {
    /// Creates new task from future, pinning it inside the box.
    /// Use this when future type is not `Unpin`,
    /// or when it has to be erased.
    pub fn pin<F>(fut: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Task::new(Box::pin(fut))
    }
}

trait AnyTask: Send + 'static {
    fn poll(&mut self, id: EntityId, queue: Arc<Mutex<Vec<(EntityId, TypeId)>>>) -> Poll<()>;
}

impl<T, F> AnyTask for Task<T, F>
where
    T: 'static,
    F: Future<Output = ()> + Unpin + Send + 'static,
{
    fn poll(&mut self, id: EntityId, queue: Arc<Mutex<Vec<(EntityId, TypeId)>>>) -> Poll<()> {
        let waker = TaskWaker::<Self>::waker(id, queue);
        let mut cx = Context::from_waker(&waker);
        Pin::new(&mut self.fut).poll(&mut cx)
    }
}

impl<T, F> Component for Task<T, F>
where
    T: 'static,
    F: Future<Output = ()> + Unpin + Send + 'static,
{
    fn borrows() -> Vec<ComponentBorrow> {
        let mut borrows = vec![ComponentBorrow::auto::<Self>()];
        borrow_dyn_trait!(Self as AnyTask => borrows);
        borrows
    }
}

use alloc::{sync::Arc, task::Wake};
use parking_lot::Mutex;
use tls::WorldTLS;

/// State of [`task_system`].
#[derive(Default)]
pub struct TaskSystemState {
    queue: Arc<Mutex<Vec<(EntityId, TypeId)>>>,
    wakes: Vec<(EntityId, TypeId)>,
    finished: Vec<(EntityId, TypeId)>,
    after_epoch: EpochId,
}

/// Access world inside async task executed by [`task_system`].
pub fn task_world<R>(f: impl FnOnce(&mut World) -> R) -> R {
    // Safety: Reference do not escape local scope.
    unsafe { f(tls::WorldTLS::get()) }
}

/// System that executes async tasks.
///
/// This system relies on TLS,
/// if `task_system` runs inside another `task_world` call uses
/// [`World`] from most inner `task_system`.
///
/// # Example
///
/// ```
/// # use edict::{world::World, scheduler::Scheduler, system::{IntoSystem, Res}, task::{Task, task_system, task_world}};
///
/// let mut world = World::new();
/// let mut scheduler = Scheduler::new();
///
/// struct Yield(bool);
///
/// impl std::future::Future for Yield {
///   type Output = ();
///
///   fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context) -> std::task::Poll<()> {
///     if std::mem::replace(&mut self.get_mut().0, true) {
///       std::task::Poll::Ready(())
///     } else {
///       cx.waker().wake_by_ref();
///       std::task::Poll::Pending
///     }
///   }
/// }
///
/// fn yield_once() -> Yield {
///   Yield(false)
/// }
///
/// world.spawn((Task::<()>::pin(async {
///   loop {
///     let stop = task_world(|world| {
///       let r = world.get_resource_mut::<i32>();
///       if let Some(mut r) = r {
///         assert_eq!(0, *r);
///         *r = 1;
///         true
///       } else {
///         false
///       }
///     });
///     if stop {
///       return;
///     }
///
///     yield_once().await;
///   }
/// }),));
///
/// scheduler.add_system(task_system);
///
/// for _ in 0..5 {
///   scheduler.run_sequential(&mut world);
/// }
///
/// world.insert_resource(0i32);
///
/// scheduler.run_sequential(&mut world);
///
/// assert_eq!(1, *world.expect_resource::<i32>());
/// ```
pub fn task_system(world: &mut World, mut state: State<TaskSystemState>) {
    let state = &mut *state;
    let after_epoch = state.after_epoch;
    state.after_epoch = world.epoch();

    let guard_tls = WorldTLS::new(world);

    for archetype in world.archetypes() {
        let Some(indices) = archetype.borrow_mut_indices(type_id::<dyn AnyTask>()) else {
            continue;
        };
        for &(tid, borrow_idx) in indices {
            let component = unsafe { archetype.component(tid).unwrap_unchecked() };
            let data = unsafe { component.data_mut() };

            if !data.epoch.after(after_epoch) {
                continue;
            }
            let borrow = component.borrows()[borrow_idx];

            let mut indices = 0..archetype.len() as u32;

            while let Some(entity_idx) = indices.next() {
                if let Some(chunk_idx) = first_of_chunk(entity_idx) {
                    if !data.chunk_epochs[chunk_idx as usize].after(after_epoch) {
                        indices.nth(CHUNK_LEN as usize - 1);
                        continue;
                    }
                }
                if !data.entity_epochs[entity_idx as usize].after(after_epoch) {
                    continue;
                }

                let ptr = unsafe {
                    NonNull::new_unchecked(
                        data.ptr
                            .as_ptr()
                            .add(component.layout().size() * (entity_idx as usize)),
                    )
                };

                let task = unsafe {
                    borrow.borrow_mut::<dyn AnyTask>().unwrap_unchecked()(ptr, PhantomData)
                };

                let id = archetype.entities()[entity_idx as usize];
                match task.poll(id, state.queue.clone()) {
                    Poll::Pending => {}
                    Poll::Ready(()) => {
                        state.finished.push((id, tid));
                    }
                }
            }
        }
    }

    core::mem::swap(&mut state.wakes, &mut state.queue.lock());

    for (id, tid) in state.wakes.drain(..) {
        let Some(loc) = world.entities().get_location(id) else {
            continue;
        };
        let arch = &world.archetypes()[loc.arch as usize];
        let Some(component) = arch.component(tid) else {
            continue;
        };
        let borrow = unsafe {
            component
                .borrows()
                .iter()
                .find(|b| b.target() == type_id::<dyn AnyTask>())
                .unwrap_unchecked()
        };
        let data = unsafe { component.data_mut() };

        let ptr = unsafe {
            NonNull::new_unchecked(
                data.ptr
                    .as_ptr()
                    .add(component.layout().size() * loc.idx as usize),
            )
        };

        let task =
            unsafe { borrow.borrow_mut::<dyn AnyTask>().unwrap_unchecked()(ptr, PhantomData) };
        match task.poll(id, state.queue.clone()) {
            Poll::Pending => {}
            Poll::Ready(()) => {
                state.finished.push((id, tid));
            }
        }
    }

    drop(guard_tls);

    for (id, tid) in state.finished.drain(..) {
        let _ = world.drop_erased(id, tid);
    }
}

#[test]
fn test_task_system() {
    use edict::{
        scheduler::Scheduler,
        task::{task_system, task_world, Task},
        world::World,
    };

    let mut world = World::new();
    let mut scheduler = Scheduler::new();

    struct Yield(bool);

    impl core::future::Future for Yield {
        type Output = ();

        fn poll(
            self: core::pin::Pin<&mut Self>,
            cx: &mut core::task::Context,
        ) -> core::task::Poll<()> {
            if core::mem::replace(&mut self.get_mut().0, true) {
                core::task::Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                core::task::Poll::Pending
            }
        }
    }

    fn yield_once() -> Yield {
        Yield(false)
    }

    world.spawn((Task::<()>::pin(async {
        loop {
            let stop = task_world(|world| {
                let r = world.get_resource_mut::<i32>();
                if let Some(mut r) = r {
                    assert_eq!(0, *r);
                    *r = 1;
                    true
                } else {
                    false
                }
            });
            if stop {
                return;
            }

            yield_once().await;
        }
    }),));

    scheduler.add_system(task_system);

    for _ in 0..5 {
        scheduler.run_sequential(&mut world);
    }

    world.insert_resource(0i32);

    scheduler.run_sequential(&mut world);

    assert_eq!(1, *world.expect_resource::<i32>());
}
