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

use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};

use hashbrown::HashSet;

use crate::{
    action::ActionEncoder,
    executor::ScopedExecutor,
    query::Access,
    system::{ActionQueue, IntoSystem, System},
    world::World,
};

/// Scheduler that starts systems in order of their registration.
/// And executes as many non-conflicting systems in parallel as possible.
pub struct Scheduler {
    systems: Vec<ScheduledSystem>,
    schedule_cache_id: Option<u64>,
    encoders: Vec<ActionEncoder>,
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

#[derive(Clone)]
struct MyActionQueue {
    encoder_rx: flume::Receiver<ActionEncoder>,
    encoder_tx: flume::Sender<ActionEncoder>,
}

impl ActionQueue for MyActionQueue {
    #[inline]
    fn get_action_encoder(&self) -> ActionEncoder {
        match self.encoder_rx.try_recv() {
            Err(_) => ActionEncoder::new(),
            Ok(encoder) => encoder,
        }
    }

    #[inline]
    fn flush_action_encoder(&mut self, encoder: ActionEncoder) {
        self.encoder_tx.send(encoder).unwrap();
    }
}

struct Task<'scope> {
    system_idx: usize,
    systems: &'scope [ScheduledSystem],
    world: &'scope World,
    task_tx: flume::Sender<Task<'scope>>,
    action_queue: MyActionQueue,
}

impl<'scope> Task<'scope> {
    fn run(self, executor: &impl ScopedExecutor<'scope>) {
        let Task {
            system_idx,
            systems,
            world,
            task_tx,
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
                system.run_unchecked(world, &mut action_queue);
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
                            task_tx: task_tx.clone(),
                            action_queue: action_queue.clone(),
                        };
                        if is_local {
                            task_tx.send(task).unwrap();
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
            encoders: Vec::new(),
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

    /// Runs all systems in the scheduler.
    /// Provided closure should spawn system execution task.
    ///
    /// Running systems on the current thread instead can be viable for debugging purposes.
    #[must_use]
    pub fn run<'scope, 'later: 'scope>(
        &'later mut self,
        world: &'scope mut World,
        executor: &impl ScopedExecutor<'scope>,
    ) -> &'later mut [ActionEncoder] {
        let world: &'scope World = &*world;

        if self.schedule_cache_id != Some(world.archetype_set_id()) {
            // Re-schedule systems for new archetypes set.
            self.reschedule(world);
        }

        for system in &mut self.systems {
            *system.wait.get_mut() = system.dependencies;
        }

        let (task_tx, task_rx) = flume::bounded(self.systems.len());
        let (encoder_tx, encoder_rx) = flume::unbounded();

        for encoder in self.encoders.drain(..) {
            encoder_tx.send(encoder).unwrap();
        }

        let action_queue = MyActionQueue {
            encoder_rx,
            encoder_tx,
        };

        let mut unroll = None;

        for (idx, system) in self.systems.iter().enumerate() {
            let old = system.wait.fetch_sub(1, Ordering::Acquire);
            if old == 0 {
                let is_local = system.is_local;
                let task = Task {
                    system_idx: idx,
                    world,
                    systems: &self.systems,
                    task_tx: task_tx.clone(),
                    action_queue: action_queue.clone(),
                };
                if is_local && unroll.is_none() {
                    unroll = Some(task);
                } else {
                    executor.spawn(move |executor| task.run(executor));
                }
            }
        }

        drop(task_tx);

        if let Some(task) = unroll {
            task.run(executor);
        }

        while let Ok(task) = task_rx.recv() {
            task.run(executor);
        }

        let MyActionQueue {
            encoder_rx,
            encoder_tx,
        } = action_queue;

        drop(encoder_tx);

        while let Ok(encoder) = encoder_rx.recv() {
            self.encoders.push(encoder);
        }

        &mut self.encoders[..]
    }

    fn reschedule(&mut self, world: &World) {
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

                for id in world.resource_types() {
                    match (system_a.access_resource(id), system_b.access_resource(id)) {
                        (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write)) => {
                            // Conflicts on this resource.
                            // Add a dependency.
                            self.systems[j].dependents.push(i);
                            self.systems[i].dependencies += 1;
                            deps.insert(j);
                            continue 'j;
                        }
                        _ => {}
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

                    if system_a.skips_archetype(archetype) || system_b.skips_archetype(archetype) {
                        // Ignore skipped archetypes.
                        continue;
                    }

                    for info in archetype.infos() {
                        match (
                            system_a.access_component(info.id()),
                            system_b.access_component(info.id()),
                        ) {
                            (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write)) => {
                                // Conflicts on this archetype.
                                // Add a dependency.
                                self.systems[j].dependents.push(i);
                                self.systems[i].dependencies += 1;
                                deps.insert(j);
                                continue 'j;
                            }
                            _ => {}
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

    use crate::{
        action::ActionEncoderSliceExt, component::Component, executor::MockExecutor, system::State,
    };
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

        scheduler
            .run(&mut world, &MockExecutor)
            .execute_all(&mut world);
    }
}
