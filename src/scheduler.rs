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
    sync::atomic::{AtomicUsize, Ordering},
};

use hashbrown::HashSet;

use crate::{
    query::Access,
    system::{IntoSystem, System},
    world::World,
};

/// Scheduler that starts systems in order of their registration.
/// And executes as many non-conflicting systems in parallel as possible.
pub struct Scheduler {
    systems: Vec<ScheduledSystem>,
    schedule_cache_id: Option<u64>,
}

struct ScheduledSystem {
    system: UnsafeCell<Box<dyn System>>,
    dependents: Vec<usize>,
    dependencies: usize,
    wait: AtomicUsize,
}

pub struct Task<'a> {
    system: &'a mut dyn System,
    world: &'a World,
    dependents: &'a [usize],
    systems: &'a [ScheduledSystem],
    spawn: &'a dyn Fn(Self),
}

impl<'a> Task<'a> {
    fn run(self) {
        let mut unroll = Some(self.system);

        while let Some(system) = unroll.take() {
            system.run(self.world);

            for &d in self.dependents {
                let old = self.systems[d].wait.fetch_sub(1, Ordering::AcqRel);
                if old == 0 {
                    let system = unsafe {
                        // # Safety
                        //
                        // Only task that decrements zeroed wait counter gets to run the system.
                        &mut **self.systems[d].system.get()
                    };
                    if unroll.is_none() {
                        unroll = Some(system);
                    } else {
                        (self.spawn)(Task {
                            system,
                            world: self.world,
                            dependents: &self.systems[d].dependents,
                            systems: self.systems,
                            spawn: self.spawn,
                        });
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
        }
    }

    /// Adds system to the scheduler.
    pub fn add_system<M>(&mut self, system: impl IntoSystem<M>) {
        self.add_boxed_system(Box::new(system.into_system()));
    }

    /// Adds system to the scheduler.
    ///
    /// ```
    /// use edict::scheduler::{Scheduler, System};
    ///
    /// struct MySystem;
    ///
    /// impl System<()> for MySystem {
    ///     fn run(&mut self, cx: ()) {}
    /// }
    ///
    /// let mut scheduler = Scheduler::new();
    /// let system: Box<dyn System<()>> = Box::new(MySystem);
    /// scheduler.add_boxed_system(system);
    /// ```
    pub fn add_boxed_system(&mut self, system: Box<dyn System>) {
        self.systems.push(ScheduledSystem {
            system: UnsafeCell::new(system),
            dependents: Vec::new(),
            dependencies: 0,
            wait: AtomicUsize::new(0),
        });
        self.schedule_cache_id = None;
    }

    /// Runs all systems in the scheduler.
    /// Provided closure should spawn system execution task.
    ///
    /// Running systems on the current thread instead can be viable for debugging purposes.
    pub fn run(&mut self, world: &World, spawn: impl Fn(Task<'_>)) {
        if self.schedule_cache_id != Some(world.archetype_set_id()) {
            // Re-schedule systems for new archetypes set.
            self.reschedule(world);
        }

        for system in &mut self.systems {
            system.wait.store(system.dependencies, Ordering::Relaxed);
        }

        let mut unroll = None;

        for system in &self.systems {
            let old = system.wait.fetch_sub(1, Ordering::Acquire);
            if old == 0 {
                let task = Task {
                    system: unsafe { &mut **system.system.get() },
                    world,
                    dependents: &system.dependents,
                    systems: &self.systems,
                    spawn: &spawn,
                };
                if unroll.is_none() {
                    unroll = Some(task);
                } else {
                    (spawn)(task);
                }
            }
        }

        if let Some(task) = unroll {
            task.run();
        }
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

                for archetype in world.archetypes() {
                    let a = unsafe {
                        // # Safety
                        //
                        // Unique access to systems.
                        &*a.system.get()
                    };

                    let b = unsafe {
                        // # Safety
                        //
                        // Unique access to systems.
                        // j is always less than i
                        &*b.system.get()
                    };

                    if a.skips_archetype(archetype) || b.skips_archetype(archetype) {
                        // Ignore skipped archetypes.
                        continue;
                    }

                    for info in archetype.infos() {
                        match (a.access_component(info.id()), b.access_component(info.id())) {
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

    use crate::{component::Component, system::State};
    struct Foo;

    impl Component for Foo {}

    #[test]
    fn test() {
        let mut world = World::new();

        let mut scheduler = Scheduler::new();
        scheduler.add_system(|q: &mut State<i32>| {
            **q = 11;
        });
    }
}
