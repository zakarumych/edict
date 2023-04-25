//! Built-in scheduling for systems.
//!
//! Users are free to use their own scheduling.
//!
//! Built-in [`Scheduler`] has following properties:
//! * Separates execution of conflicting systems temporally.
//! * Executes non-conflicting systems in parallel on available worker threads.
//! * Conflicting systems are executed in order of their registration.
//!   That means that system defines implicit dependency on all systems with with there's conflict.
//!   * In case of write-to-read conflict, reading system that is added later is guaranteed
//!     to observe modifications made by writing system that was added before.
//!   * In case of read-to-write conflict, reading system that is added before is guaranteed
//!     to NOT observe modifications made by writing system that was added later.
//!   * In case of write-to-write conflict, writing system that is added before is guaranteed
//!     to NOT observe modifications made by writing system that was added later.
//!     And writing system that is added later is guaranteed
//!     to observe modifications made by writing system that was added before.
//!

#![allow(missing_docs)]

use alloc::{collections::VecDeque, sync::Arc};
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};
use std::thread::Thread;

use hashbrown::HashSet;
use parking_lot::Mutex;

use crate::{
    action::ActionBuffer,
    executor::{MockExecutor, ScopedExecutor},
    query::Access,
    system::{ActionQueue, IntoSystem, System},
    world::World,
};

/// Scheduler that starts systems in order of their registration.
/// And executes as many non-conflicting systems in parallel as possible.
///
/// # Example
///
/// ```
/// # use edict::{world::World, scheduler::Scheduler, system::{IntoSystem, Res}};
///
/// let mut world = World::new();
/// let mut scheduler = Scheduler::new();
///
/// scheduler.add_system(|| {});
/// scheduler.add_system(|world: &mut World| {
///   println!("{}", world.with_resource::<i32>(|| 0));
/// });
/// scheduler.add_system(|world: &World| {
///   assert_eq!(0, *world.expect_resource::<i32>());
/// });
/// scheduler.add_system(|res: Res<i32>| {
///   assert_eq!(0, *res);
/// });
///
/// scheduler.run_threaded(&mut world);
/// ```
pub struct Scheduler {
    systems: Vec<ScheduledSystem>,
    schedule_cache_id: Option<u64>,
    action_buffers: Vec<ActionBuffer>,
}

struct SyncUnsafeCell<T: ?Sized> {
    inner: UnsafeCell<T>,
}

impl<T> SyncUnsafeCell<T> {
    pub fn new(value: T) -> Self {
        SyncUnsafeCell {
            inner: UnsafeCell::new(value),
        }
    }
}

unsafe impl<T: ?Sized> Sync for SyncUnsafeCell<T> {}

impl<T: ?Sized> Deref for SyncUnsafeCell<T> {
    type Target = UnsafeCell<T>;

    fn deref(&self) -> &UnsafeCell<T> {
        &self.inner
    }
}

impl<T: ?Sized> DerefMut for SyncUnsafeCell<T> {
    fn deref_mut(&mut self) -> &mut UnsafeCell<T> {
        &mut self.inner
    }
}

struct ScheduledSystem {
    system: SyncUnsafeCell<Box<dyn System + Send>>,
    wait: AtomicUsize,
    dependents: Vec<usize>,
    dependencies: usize,
    is_local: bool,
}

struct Queue<T> {
    items: Mutex<VecDeque<T>>,
    thread: Thread,
}

impl<T> Queue<T> {
    fn new() -> Self {
        Queue {
            items: Mutex::new(VecDeque::new()),
            thread: std::thread::current(),
        }
    }

    fn enqueue(&self, item: T) {
        self.items.lock().push_back(item);
        self.thread.unpark();
    }

    fn try_deque(&self) -> Option<T> {
        self.items.lock().pop_front()
    }

    fn deque(self: &Arc<Self>) -> Result<T, ()> {
        let me: &Self = self;
        loop {
            if Arc::strong_count(self) == 1 {
                return Err(());
            }
            if let Some(item) = me.try_deque() {
                return Ok(item);
            }
            std::thread::park();
        }
    }
}

impl ActionQueue for Arc<Queue<ActionBuffer>> {
    #[inline]
    fn get<'a>(&self) -> ActionBuffer {
        match self.try_deque() {
            Some(buffer) => buffer,
            None => ActionBuffer::new(),
        }
    }

    #[inline]
    fn flush(&mut self, buffer: ActionBuffer) {
        self.enqueue(buffer);
    }
}

#[derive(Clone, Copy)]
struct NonNullWorld {
    ptr: NonNull<World>,
}

unsafe impl Send for NonNullWorld {}

struct Task<'scope> {
    system_idx: usize,
    systems: &'scope [ScheduledSystem],
    world: NonNullWorld,
    task_queue: Arc<Queue<Task<'scope>>>,
    action_queue: Arc<Queue<ActionBuffer>>,
}

impl<'scope> Task<'scope> {
    fn run(self, executor: &impl ScopedExecutor<'scope>) {
        let Task {
            system_idx,
            systems,
            world,
            task_queue,
            mut action_queue,
        } = self;

        let mut dependents = &systems[system_idx].dependents[..];
        let mut unroll = Some(unsafe {
            // # Safety
            //
            // Only spawned task gets to run this system.
            &mut **systems[system_idx].system.get()
        });

        while let Some(system) = unroll.take() {
            unsafe {
                system.run_unchecked(world.ptr, &mut action_queue);
            }

            for &dependent_idx in dependents {
                let old = systems[dependent_idx].wait.fetch_sub(1, Ordering::AcqRel);
                if old == 0 {
                    let is_local = systems[dependent_idx].is_local;

                    if !is_local && unroll.is_none() {
                        unroll = Some(unsafe {
                            // # Safety
                            //
                            // Only task that decrements zeroed wait counter gets to run this system.
                            &mut **systems[dependent_idx].system.inner.get()
                        });
                        dependents = &systems[dependent_idx].dependents[..];
                    } else {
                        let task = Task {
                            system_idx: dependent_idx,
                            systems: systems,
                            world: world,
                            task_queue: task_queue.clone(),
                            action_queue: action_queue.clone(),
                        };
                        if is_local {
                            task_queue.enqueue(task);
                        } else {
                            executor.spawn(move |executor| task.run(executor));
                        }
                    }
                }
            }
        }
    }
}

impl Scheduler {
    /// Creates new empty scheduler.
    pub fn new() -> Self {
        Scheduler {
            systems: Vec::new(),
            schedule_cache_id: None,
            action_buffers: Vec::new(),
        }
    }

    /// Adds system to the scheduler.
    pub fn add_system<M>(&mut self, system: impl IntoSystem<M>) {
        self.add_boxed_system(Box::new(system.into_system()));
    }

    /// Adds system to the scheduler.
    pub fn add_boxed_system(&mut self, system: Box<dyn System + Send>) {
        self.systems.push(ScheduledSystem {
            is_local: system.is_local(),
            system: SyncUnsafeCell::new(system),
            wait: AtomicUsize::new(0),
            dependents: Vec::new(),
            dependencies: 0,
        });
        self.schedule_cache_id = None;
    }

    #[cfg(feature = "std")]
    pub fn run_threaded(&mut self, world: &mut World) {
        use crate::action::ActionBufferSliceExt;
        let buffers = std::thread::scope(|scope| self.run_with(world, &scope));
        buffers.execute_all(world);
    }

    #[cfg(feature = "rayon")]
    pub fn run_rayon(&mut self, world: &mut World) {
        use crate::action::ActionBufferSliceExt;
        let buffers = rayon::in_place_scope(|scope| self.run_with(world, scope));
        buffers.execute_all(world);
    }

    pub fn run_sequential(&mut self, world: &mut World) {
        use crate::action::ActionBufferSliceExt;
        let buffers = self.run_with(world, &mut MockExecutor);
        buffers.execute_all(world);
    }

    /// Runs all systems in the scheduler.
    /// Provided closure should spawn system execution task.
    ///
    /// Running systems on the current thread instead can be viable for debugging purposes.
    #[must_use]
    pub fn run_with<'scope, 'later: 'scope>(
        &'later mut self,
        world: &'scope mut World,
        executor: &impl ScopedExecutor<'scope>,
    ) -> &'later mut [ActionBuffer] {
        self.reschedule(world);

        for system in &mut self.systems {
            *system.wait.get_mut() = system.dependencies;
        }

        let task_queue = Arc::new(Queue::new());
        let action_queue = Arc::new(Queue::new());

        for buffer in self.action_buffers.drain(..) {
            action_queue.enqueue(buffer);
        }

        let mut unroll = None;

        let world_ptr = NonNull::from(world);

        for (idx, system) in self.systems.iter().enumerate() {
            let old = system.wait.fetch_sub(1, Ordering::Acquire);
            if old == 0 {
                let is_local = system.is_local;
                let task = Task {
                    system_idx: idx,
                    world: NonNullWorld { ptr: world_ptr },
                    systems: &self.systems,
                    task_queue: task_queue.clone(),
                    action_queue: action_queue.clone(),
                };
                if is_local && unroll.is_none() {
                    unroll = Some(task);
                } else {
                    executor.spawn(move |executor| task.run(executor));
                }
            }
        }

        if let Some(task) = unroll {
            task.run(executor);
        }

        while let Ok(task) = task_queue.deque() {
            task.run(executor);
        }

        while let Ok(buffer) = action_queue.deque() {
            self.action_buffers.push(buffer);
        }

        &mut self.action_buffers[..]
    }

    fn reschedule(&mut self, world: &World) {
        if self.schedule_cache_id == Some(world.archetype_set_id()) {
            return;
        }

        for i in 0..self.systems.len() {
            // Reset dependencies.
            let a = &mut self.systems[i];
            a.dependents.clear();
            a.dependencies = 0;

            let mut deps = HashSet::new();

            'j: for j in (0..i).rev() {
                let a = &self.systems[i];
                let b = &self.systems[j];

                for &d in &b.dependents {
                    if deps.contains(&d) {
                        // A transitive dependency.
                        deps.insert(j);
                        continue 'j;
                    }
                }

                let system_a = unsafe {
                    // # Safety
                    //
                    // Unique access to systems.
                    &*a.system.get()
                };

                let system_b = unsafe {
                    // # Safety
                    //
                    // Unique access to systems.
                    // j is always less than i
                    &*b.system.get()
                };

                if conflicts(system_a.world_access(), system_b.world_access()) {
                    // Conflicts on world access.
                    // Add a dependency.
                    self.systems[j].dependents.push(i);
                    self.systems[i].dependencies += 1;
                    deps.insert(j);
                    continue 'j;
                }

                for id in world.resource_types() {
                    if conflicts(system_a.access_resource(id), system_b.access_resource(id)) {
                        // Conflicts on this resource.
                        // Add a dependency.
                        self.systems[j].dependents.push(i);
                        self.systems[i].dependencies += 1;
                        deps.insert(j);
                        continue 'j;
                    }
                }

                for archetype in world.archetypes() {
                    let system_a = unsafe {
                        // # Safety
                        //
                        // Unique access to systems.
                        &*a.system.get()
                    };

                    let system_b = unsafe {
                        // # Safety
                        //
                        // Unique access to systems.
                        // j is always less than i
                        &*b.system.get()
                    };

                    if !system_a.visit_archetype(archetype) || !system_b.visit_archetype(archetype)
                    {
                        // Ignore skipped archetypes.
                        continue;
                    }

                    for info in archetype.infos() {
                        if conflicts(
                            system_a.access_component(info.id()),
                            system_b.access_component(info.id()),
                        ) {
                            // Conflicts on this archetype.
                            // Add a dependency.
                            self.systems[j].dependents.push(i);
                            self.systems[i].dependencies += 1;
                            deps.insert(j);
                            continue 'j;
                        }
                    }
                }
            }
        }
    }
}

mod test {
    #![cfg(test)]

    use super::*;

    use crate::{component::Component, system::State};
    struct Foo;

    impl Component for Foo {}

    #[test]
    fn test() {
        let mut world = World::new();

        let mut scheduler = Scheduler::new();
        scheduler.add_system(|mut q: State<i32>| {
            *q = 11;
            println!("{}", *q);
        });

        scheduler.run_sequential(&mut world);
    }
}

fn conflicts(lhs: Option<Access>, rhs: Option<Access>) -> bool {
    matches!(
        (lhs, rhs),
        (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write))
    )
}
