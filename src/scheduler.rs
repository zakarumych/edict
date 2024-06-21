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

use alloc::{boxed::Box, collections::VecDeque, sync::Arc, vec::Vec};
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

use hashbrown::HashSet;

#[derive(Clone)]
#[repr(transparent)]
struct Thread {
    #[cfg(feature = "std")]
    thread: std::thread::Thread,

    #[cfg(not(feature = "std"))]
    thread: *mut u8,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    pub fn unpark(&self) {
        #[cfg(feature = "std")]
        self.thread.unpark();

        #[cfg(not(feature = "std"))]
        unsafe {
            edict_unpark_thread(self.thread);
        }
    }

    pub fn current() -> Self {
        #[cfg(feature = "std")]
        let thread = std::thread::current();

        #[cfg(not(feature = "std"))]
        let thread = unsafe { edict_current_thread() };

        Thread { thread }
    }

    pub fn park() {
        #[cfg(feature = "std")]
        std::thread::park();

        #[cfg(not(feature = "std"))]
        unsafe {
            edict_park_thread();
        }
    }
}

#[cfg(feature = "std")]
use parking_lot::Mutex;

#[cfg(not(feature = "std"))]
pub struct CurrentThread;

#[cfg(not(feature = "std"))]
impl amity::park::DefaultPark for Thread {
    type Park = CurrentThread;
    fn default_park() -> CurrentThread {
        CurrentThread
    }
}

#[cfg(not(feature = "std"))]
impl amity::park::Park<Thread> for CurrentThread {
    fn unpark_token(&self) -> Thread {
        Thread::current()
    }

    fn park(&self) {
        Thread::park();
    }
}

#[cfg(not(feature = "std"))]
impl amity::park::Unpark for Thread {
    fn unpark(&self) {
        self.unpark();
    }
}

#[cfg(not(feature = "std"))]
type Mutex<T> = lock_api::Mutex<amity::mutex::RawMutex<Thread>, T>;

use crate::{
    action::ActionBuffer,
    executor::ScopedExecutor,
    system::ActionBufferQueue,
    system::{IntoSystem, System},
    world::World,
    Access,
};

#[cfg(not(feature = "std"))]
use crate::nostd::scheduler::{edict_current_thread, edict_park_thread, edict_unpark_thread};

/// Scheduler that starts systems in order of their registration.
/// And executes as many non-conflicting systems in parallel as possible.
///
/// # Example
///
/// ```
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

struct QueueInner<T> {
    items: Mutex<VecDeque<T>>,
    thread: Thread,
}

struct Queue<T> {
    inner: Arc<QueueInner<T>>,
}

impl<T> Clone for Queue<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Queue {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Drop for Queue<T> {
    #[inline(always)]
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 2 {
            self.inner.thread.unpark();
        }
    }
}

impl<T> Queue<T> {
    #[inline(always)]
    fn new() -> Self {
        Queue {
            inner: Arc::new(QueueInner {
                items: Mutex::new(VecDeque::new()),
                thread: Thread::current(),
            }),
        }
    }

    #[inline(always)]
    fn enqueue(&self, item: T) {
        self.inner.items.lock().push_back(item);
        self.inner.thread.unpark();
    }

    #[inline(always)]
    fn try_deque(&self) -> Option<T> {
        self.inner.items.lock().pop_front()
    }

    #[inline(always)]
    fn deque(&self) -> Result<T, ()> {
        loop {
            if let Some(item) = self.try_deque() {
                return Ok(item);
            }
            if Arc::strong_count(&self.inner) == 1 {
                return Err(());
            }
            Thread::park();
        }
    }
}

impl ActionBufferQueue for Queue<ActionBuffer> {
    #[inline(always)]
    fn get<'a>(&self) -> ActionBuffer {
        // Taking last ensures that commands from system won't be executed before commands
        // of its transient dependencies.
        if let Some(last) = self.inner.items.lock().pop_back() {
            last
        } else {
            ActionBuffer::new()
        }
    }

    #[inline(always)]
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
    task_queue: Queue<Task<'scope>>,
    action_queue: Queue<ActionBuffer>,
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

    /// Runs all systems in the scheduler using std threads.
    #[cfg(feature = "std")]
    pub fn run_threaded(&mut self, world: &mut World) {
        use crate::action::ActionBufferSliceExt;
        let buffers = std::thread::scope(|scope| self.run_with(world, &scope));
        buffers.execute_all(world);
    }

    /// Runs all systems in the scheduler using rayon.
    #[cfg(feature = "rayon")]
    pub fn run_rayon(&mut self, world: &mut World) {
        use crate::action::ActionBufferSliceExt;
        let buffers = rayon::in_place_scope(|scope| self.run_with(world, scope));
        buffers.execute_all(world);
    }

    /// Runs all systems in the scheduler sequentially.
    pub fn run_sequential(&mut self, world: &mut World) {
        use crate::action::ActionBufferSliceExt;
        // let buffers = self.run_with(world, &mut MockExecutor);
        // buffers.execute_all(world);

        let mut buffers = Vec::new();
        {
            for system in &mut self.systems {
                let system = system.system.inner.get_mut();
                unsafe {
                    system.run_unchecked(NonNull::from(&mut *world), &mut buffers);
                }
            }
        }
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

        let task_queue = Queue::new();
        let action_queue = Queue::new();

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
                if is_local {
                    if unroll.is_none() {
                        unroll = Some(task);
                    } else {
                        task_queue.enqueue(task);
                    }
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
                    if conflicts(
                        system_a.resource_type_access(id),
                        system_b.resource_type_access(id),
                    ) {
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
                            system_a.component_access(info),
                            system_b.component_access(info),
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

fn conflicts(lhs: Option<Access>, rhs: Option<Access>) -> bool {
    matches!(
        (lhs, rhs),
        (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write))
    )
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
