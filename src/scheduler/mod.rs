//! Built-in scheduling for systems.
//!
//! [`Scheduler`] has following properties:
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

use alloc::{boxed::Box, vec::Vec};
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use amity::ring_buffer::RingBuffer;

use crate::{
    action::ActionBuffer,
    system::{IntoSystem, System},
    world::World,
};

#[cfg(feature = "threaded-scheduler")]
mod threaded;

#[cfg(feature = "threaded-scheduler")]
pub use self::threaded::ScopedExecutor;

/// Scheduler that starts systems in order of their registration.
/// And executes as many non-conflicting systems in parallel as possible.
///
/// # Example
///
/// ```
/// # #[cfg(feature = "threaded-scheduler")]
/// # {
/// # use edict::{world::World, resources::Res, scheduler::Scheduler, system::IntoSystem};
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
/// # }
/// ```
pub struct Scheduler {
    systems: Vec<ScheduledSystem>,
    action_buffers: RingBuffer<ActionBuffer>,

    #[cfg(feature = "threaded-scheduler")]
    schedule_cache_id: Option<u64>,
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

    #[cfg(feature = "threaded-scheduler")]
    threaded: self::threaded::ThreadedSystem,
}

impl Scheduler {
    /// Creates new empty scheduler.
    pub fn new() -> Self {
        Scheduler {
            systems: Vec::new(),
            action_buffers: RingBuffer::new(),

            #[cfg(feature = "threaded-scheduler")]
            schedule_cache_id: None,
        }
    }

    /// Adds system to the scheduler.
    pub fn add_system<M>(&mut self, system: impl IntoSystem<M>) {
        self.add_boxed_system(Box::new(system.into_system()));
    }

    /// Adds system to the scheduler.
    pub fn add_boxed_system(&mut self, system: Box<dyn System + Send>) {
        self.systems.push(ScheduledSystem {
            #[cfg(feature = "threaded-scheduler")]
            threaded: self::threaded::ThreadedSystem::new(system.is_local()),

            system: SyncUnsafeCell::new(system),
        });

        #[cfg(feature = "threaded-scheduler")]
        {
            self.schedule_cache_id = None;
        }
    }

    /// Runs all systems in the scheduler sequentially.
    pub fn run_sequential(&mut self, world: &mut World) {
        use crate::action::ActionBufferSliceExt;

        for system in &mut self.systems {
            let system = system.system.inner.get_mut();
            unsafe {
                system.run_unchecked(NonNull::from(&mut *world), &mut self.action_buffers);
            }
        }

        let (front, back) = self.action_buffers.as_mut_slices();
        front.execute_all(world);
        back.execute_all(world);
    }
}

#[cfg(test)]
mod test {

    use super::*;

    use crate::system::State;

    #[test]
    fn test() {
        let mut world = World::new();

        let mut scheduler = Scheduler::new();
        scheduler.add_system(|mut q: State<i32>| {
            *q = 11;
        });

        scheduler.run_sequential(&mut world);
    }
}
