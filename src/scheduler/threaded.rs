//! Provides API to define task executors.

use core::ptr::NonNull;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use amity::flip_queue::FlipQueue;
use hashbrown::HashSet;

use crate::{action::ActionBuffer, system::ActionBufferQueue, world::World, Access};

use super::{ScheduledSystem, Scheduler};

#[derive(Clone, Copy)]
struct NonNullWorld {
    ptr: NonNull<World>,
}

unsafe impl Send for NonNullWorld {}

/// Abstract scoped task executor.
///
/// Executes provided closures potentially in parallel.
///
/// This trait is implemented for `std::thread::Scope` when the `std` feature is enabled,
/// and for `rayon::Scope` when the `rayon` feature is enabled.
pub trait ScopedExecutor<'scope> {
    /// Spawns a task on the scope.
    fn spawn<F>(&self, f: F)
    where
        F: FnOnce(&Self) + Send + 'scope;
}

/// Mock executor that runs tasks on the current thread.
#[derive(Clone, Copy, Debug)]
pub struct MockExecutor;

impl<'scope> ScopedExecutor<'scope> for MockExecutor {
    fn spawn<F>(&self, f: F)
    where
        F: FnOnce(&Self) + Send + 'scope,
    {
        f(self)
    }
}

#[cfg(feature = "rayon-scheduler")]
mod rayon_scope {
    use super::ScopedExecutor;

    impl<'scope> ScopedExecutor<'scope> for rayon::Scope<'scope> {
        fn spawn<F>(&self, f: F)
        where
            F: FnOnce(&Self) + Send + 'scope,
        {
            self.spawn(f);
        }
    }
}

mod std_thread_scope {
    use std::thread;

    use super::ScopedExecutor;

    impl<'scope> ScopedExecutor<'scope> for &'scope thread::Scope<'scope, '_> {
        fn spawn<F>(&self, f: F)
        where
            F: FnOnce(&Self) + Send + 'scope,
        {
            let scope = *self;
            scope.spawn(move || {
                f(&scope);
            });
        }
    }
}

pub(super) struct ThreadedSystem {
    wait: AtomicUsize,
    dependents: Vec<usize>,
    dependencies: usize,
    is_local: bool,
}

impl ThreadedSystem {
    pub(super) fn new(is_local: bool) -> Self {
        ThreadedSystem {
            is_local,
            wait: AtomicUsize::new(0),
            dependents: Vec::new(),
            dependencies: 0,
        }
    }
}

struct Queues<'scope> {
    local_tasks: FlipQueue<Task<'scope>>,
    actions: FlipQueue<ActionBuffer>,
}

impl ActionBufferQueue for &Queues<'_> {
    #[inline(always)]
    fn get<'a>(&mut self) -> ActionBuffer {
        // Taking last ensures that commands from system won't be executed before commands
        // of its transient dependencies.
        if let Some(last) = self.actions.try_pop_sync() {
            last
        } else {
            ActionBuffer::new()
        }
    }

    #[inline(always)]
    fn flush(&mut self, buffer: ActionBuffer) {
        self.actions.push_sync(buffer);
    }
}

struct Task<'scope> {
    system_idx: usize,
    systems: &'scope [ScheduledSystem],
    world: NonNullWorld,
    queues: Arc<Queues<'scope>>,
}

impl<'scope> Task<'scope> {
    fn run(self, executor: &impl ScopedExecutor<'scope>, is_local_run: bool) {
        let Task {
            system_idx,
            systems,
            world,
            queues,
        } = self;

        let mut dependents = &systems[system_idx].threaded.dependents[..];

        // SAFETY: Only spawned task gets to run this system.
        let mut unroll = Some(unsafe { &mut **systems[system_idx].system.get() });

        while let Some(system) = unroll.take() {
            unsafe {
                system.run_unchecked(world.ptr, &mut &*queues);
            }

            for &dependent_idx in dependents {
                let old = systems[dependent_idx]
                    .threaded
                    .wait
                    .fetch_sub(1, Ordering::AcqRel);
                if old == 0 {
                    let is_local = systems[dependent_idx].threaded.is_local;

                    if is_local != is_local_run && unroll.is_none() {
                        unroll = Some(unsafe {
                            // # Safety
                            //
                            // Only task that decrements zeroed wait counter gets to run this system.
                            &mut **systems[dependent_idx].system.inner.get()
                        });
                        dependents = &systems[dependent_idx].threaded.dependents[..];
                    } else {
                        let task = Task {
                            system_idx: dependent_idx,
                            systems: systems,
                            world: world,
                            queues: queues.clone(),
                        };
                        if is_local {
                            queues.local_tasks.push_sync(task);
                        } else {
                            executor.spawn(move |executor| task.run(executor, false));
                        }
                    }
                }
            }
        }
    }
}

impl Scheduler {
    /// Runs all systems in the scheduler using std threads.
    pub fn run_threaded(&mut self, world: &mut World) {
        std::thread::scope(|scope| self.run_with(world, &scope));
    }

    /// Runs all systems in the scheduler using rayon.
    #[cfg(feature = "rayon-scheduler")]
    pub fn run_rayon(&mut self, world: &mut World) {
        rayon::in_place_scope(|scope| self.run_with(world, scope));
    }

    /// Runs all systems in the scheduler.
    /// Provided closure should spawn system execution task.
    ///
    /// Running systems on the current thread instead can be viable for debugging purposes.
    #[must_use]
    pub fn run_with<'scope>(
        &'scope mut self,
        world: &'scope mut World,
        executor: &impl ScopedExecutor<'scope>,
    ) {
        use crate::action::ActionBufferSliceExt;

        self.reschedule(world);

        for system in &mut self.systems {
            *system.threaded.wait.get_mut() = system.threaded.dependencies;
        }

        let task_queue: FlipQueue<Task<'scope>> = FlipQueue::with_capacity(256);
        let mut action_queue = FlipQueue::with_capacity(256);

        for buffer in self.action_buffers.drain() {
            action_queue.push(buffer);
        }

        let mut world_ptr = NonNull::from(world);

        let queues = Arc::new(Queues {
            local_tasks: task_queue,
            actions: action_queue,
        });

        for (idx, system) in self.systems.iter().enumerate() {
            let old = system.threaded.wait.fetch_sub(1, Ordering::Acquire);
            if old == 0 {
                let is_local = system.threaded.is_local;
                let task = Task::<'scope> {
                    system_idx: idx,
                    world: NonNullWorld { ptr: world_ptr },
                    systems: &self.systems,
                    queues: queues.clone(),
                };
                if is_local {
                    queues.local_tasks.push_sync(task);
                } else {
                    executor.spawn(move |executor| task.run(executor, false));
                }
            }
        }

        loop {
            while let Some(task) = queues.local_tasks.pop_sync() {
                task.run(executor, true);
            }

            if Arc::strong_count(&queues) == 1 {
                break;
            }

            std::thread::yield_now();
        }

        while let Some(buffer) = queues.actions.pop_sync() {
            self.action_buffers.push(buffer);
        }

        let (front, back) = self.action_buffers.as_mut_slices();

        // SAFETY: All spawned tasks were finished since `queues` is not shared anymore.
        unsafe {
            front.execute_all(world_ptr.as_mut());
            back.execute_all(world_ptr.as_mut());
        }
    }

    fn reschedule(&mut self, world: &World) {
        if self.schedule_cache_id == Some(world.archetype_set_id()) {
            return;
        }

        for i in 0..self.systems.len() {
            // Reset dependencies.
            let a = &mut self.systems[i];
            a.threaded.dependents.clear();
            a.threaded.dependencies = 0;

            let mut deps = HashSet::new();

            'j: for j in (0..i).rev() {
                let a = &self.systems[i];
                let b = &self.systems[j];

                for &d in &b.threaded.dependents {
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
                    self.systems[j].threaded.dependents.push(i);
                    self.systems[i].threaded.dependencies += 1;
                    deps.insert(j);
                    continue 'j;
                }

                for id in world.resource_types() {
                    if conflicts(
                        system_a.resource_type_access(id),
                        system_b.resource_type_access(id),
                    ) {
                        // Conflicts on this resource.
                        // Add a dependency.
                        self.systems[j].threaded.dependents.push(i);
                        self.systems[i].threaded.dependencies += 1;
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
                            system_a.component_access(archetype, info),
                            system_b.component_access(archetype, info),
                        ) {
                            // Conflicts on this archetype.
                            // Add a dependency.
                            self.systems[j].threaded.dependents.push(i);
                            self.systems[i].threaded.dependencies += 1;
                            deps.insert(j);
                            continue 'j;
                        }
                    }
                }
            }
        }
    }
}

fn conflicts(lhs: Option<Access>, rhs: Option<Access>) -> bool {
    matches!(
        (lhs, rhs),
        (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write))
    )
}
