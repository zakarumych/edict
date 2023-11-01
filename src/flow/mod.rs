//! Flow module provides API to create high-level async workflows on top of the Edict ECS.
//!
//! Typical example would be a flow that caues an entity to move towards a target.
//! It is resolved when the entity reaches the target or target is destroyed.
//!
//! Spawning a flow also returns a handle that can be used to await or cancel the flow.
//! Spawned flow wraps flow result in a `Result` type where `Err` signals that the flow was cancelled.
//! Flows are canceled when handle is dropped or entity is despawned.
//!
//! # Example
//!
//! ```
//! unit.move_to(target).await
//! ```
//!

use core::{
    any::TypeId,
    cell::UnsafeCell,
    future::Future,
    mem::{ManuallyDrop, MaybeUninit},
    pin::Pin,
    ptr::{addr_of, addr_of_mut, NonNull},
    task::{Context, Poll, Waker},
};

use alloc::task::Wake;

use alloc::sync::Arc;
use amity::flip_queue::FlipQueue;
use hashbrown::HashMap;
use slab::Slab;

use crate::{system::State, world::World};

/// Task that access world when polled.
pub trait Flow: Send + 'static {
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>, world: &mut World) -> Poll<()>;
}

/// Spawn a flow into the world.
///
/// The flow will be polled by the `flows_system`.
pub fn spawn_flow<F>(world: &mut World, flow: F)
where
    F: Flow,
{
    world.with_default_resource::<NewFlows>().add(flow);
}

/// Type-erased array of newly inserted flows of a single type.
trait AnyNewFlows: Send {
    /// Returns type of the flow.
    #[cfg(debug_assertions)]
    fn flow_id(&self) -> TypeId;

    /// Creates a new queue for the flow type.
    fn new_queue(&self) -> AnyQueue;

    /// Drains the array into the queue.
    unsafe fn drain(&mut self, queue: &mut AnyQueue);
}

impl<'a> dyn AnyNewFlows + 'a {
    #[inline(always)]
    unsafe fn downcast_mut<F: 'static>(&mut self) -> &mut TypedNewFlows<F> {
        #[cfg(debug_assertions)]
        assert_eq!(self.flow_id(), TypeId::of::<F>());

        unsafe { &mut *(self as *mut Self as *mut TypedNewFlows<F>) }
    }
}

type SharedFlowTask<F> = Arc<FlowTask<F>>;
type NewFlowTask<F> = Arc<MaybeUninit<FlowTask<F>>>;

/// Typed array of newly inserted flows of a single type.
struct TypedNewFlows<F> {
    array: Vec<NewFlowTask<F>>,
}

impl<F> Drop for TypedNewFlows<F> {
    fn drop(&mut self) {
        for task in self.array.drain(..) {
            // Only flow field is initialized.
            unsafe {
                let flow = addr_of!((*task.as_ptr()).flow);
                ManuallyDrop::drop(&mut *(*flow).get());
            }
        }
    }
}

impl<F> AnyNewFlows for TypedNewFlows<F>
where
    F: Flow,
{
    #[cfg(debug_assertions)]
    fn flow_id(&self) -> TypeId {
        TypeId::of::<F>()
    }

    fn new_queue(&self) -> AnyQueue {
        AnyQueue {
            queue: Arc::new(FlipQueue::new()),
            ready: Vec::new(),
            flows: Box::new(TypedFlows::<F> { array: Slab::new() }),
        }
    }

    unsafe fn drain(&mut self, queue: &mut AnyQueue) {
        let typed_flows = queue.flows.downcast_mut::<F>();

        // Reserve space to ensure oom can't happen in the loop below.
        typed_flows.array.reserve(self.array.len());

        for mut task in self.array.drain(..) {
            {
                // Safety: `task` is unique.
                let flow_mut = unsafe { Arc::get_mut(&mut task).unwrap_unchecked() };
                addr_of_mut!((*flow_mut.as_mut_ptr()).id).write(typed_flows.array.vacant_key());
                addr_of_mut!((*flow_mut.as_mut_ptr()).queue).write(queue.queue.clone());
            }
            let task = Arc::from_raw(Arc::into_raw(task) as *const FlowTask<F>);
            typed_flows.array.insert(task);
        }
    }
}

struct NewFlows {
    map: HashMap<TypeId, Box<dyn AnyNewFlows>>,
}

impl Default for NewFlows {
    fn default() -> Self {
        NewFlows {
            map: HashMap::new(),
        }
    }
}

impl NewFlows {
    pub fn add<F>(&mut self, flow: F)
    where
        F: Flow,
    {
        let new_flows = self
            .map
            .entry(TypeId::of::<F>())
            .or_insert_with(|| Box::new(TypedNewFlows::<F> { array: Vec::new() }));

        let typed_new_flows = unsafe { new_flows.downcast_mut::<F>() };

        let mut task = MaybeUninit::<FlowTask<F>>::uninit();
        unsafe {
            addr_of_mut!((*task.as_mut_ptr()).flow).write(UnsafeCell::new(ManuallyDrop::new(flow)));
        }
        typed_new_flows.array.push(Arc::new(task));
    }
}

/// Trait implemented by `TypedFlows` with `F: Flow`
trait AnyFlows: Send {
    #[cfg(debug_assertions)]
    fn flow_id(&self) -> TypeId;

    fn execute(&mut self, world: &mut World, ids: &[usize]);
}

impl dyn AnyFlows {
    #[inline(always)]
    unsafe fn downcast_mut<F: 'static>(&mut self) -> &mut TypedFlows<F> {
        #[cfg(debug_assertions)]
        assert_eq!(self.flow_id(), TypeId::of::<F>());

        unsafe { &mut *(self as *mut Self as *mut TypedFlows<F>) }
    }
}

struct FlowTask<F> {
    flow: UnsafeCell<ManuallyDrop<F>>,
    id: usize,
    queue: Arc<FlipQueue<usize>>,
}

/// Safety: `FlowTask` can be sent to another thread as `Waker`
/// which does not access `flow` field.
unsafe impl<F> Send for FlowTask<F> {}
unsafe impl<F> Sync for FlowTask<F> {}

impl<F> Wake for FlowTask<F> {
    fn wake(self: Arc<Self>) {
        self.queue.push(self.id);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.push(self.id);
    }
}

/// Container of spawned flows of specific type.
struct TypedFlows<F> {
    array: Slab<SharedFlowTask<F>>,
}

impl<F> Drop for TypedFlows<F> {
    fn drop(&mut self) {
        for task in self.array.drain() {
            unsafe {
                ManuallyDrop::drop(&mut *task.flow.get());
            }
        }
    }
}

impl<F> AnyFlows for TypedFlows<F>
where
    F: Flow,
{
    #[cfg(debug_assertions)]
    fn flow_id(&self) -> TypeId {
        TypeId::of::<F>()
    }

    fn execute(&mut self, world: &mut World, ids: &[usize]) {
        for &id in ids {
            let Some(task) = self.array.get(id) else {
                continue;
            };

            let waker = Waker::from(task.clone());
            let mut cx = Context::from_waker(&waker);

            // Safety: This is the only code that can access `task.flow`.
            // It is destroyed in-place when it is ready or TypedFlows is dropped.
            let poll = unsafe {
                let pinned = Pin::new_unchecked(&mut **task.flow.get());
                pinned.poll(&mut cx, world)
            };

            if let Poll::Ready(()) = poll {
                let task = self.array.remove(id);
                // Safety: Removed from array. `task.flow` is inaccessbile anywhere but here.
                unsafe {
                    ManuallyDrop::drop(&mut *task.flow.get());
                }
            }
        }
    }
}

/// Queue of flows of a single type.
struct AnyQueue {
    queue: Arc<FlipQueue<usize>>,
    ready: Vec<usize>,
    flows: Box<dyn AnyFlows>,
}

/// Queues of all types of flows.
struct Flows {
    map: HashMap<TypeId, AnyQueue>,
}

impl Default for Flows {
    fn default() -> Self {
        Flows {
            map: HashMap::new(),
        }
    }
}

pub fn flows_system(world: &mut World, mut flows: State<Flows>) {
    let flows = &mut *flows;

    match world.get_resource_mut::<NewFlows>() {
        None => return,
        Some(mut new_flows) => {
            for (flow_id, typed) in &mut new_flows.map {
                let queue = flows
                    .map
                    .entry(*flow_id)
                    .or_insert_with(|| typed.new_queue());

                unsafe {
                    typed.drain(queue);
                }
            }
        }
    }

    for typed in flows.map.values_mut() {
        typed.queue.drain_locking(|drain| typed.ready.extend(drain));
        typed.flows.execute(world, &typed.ready);
    }
}

/// Trait implemented by functions that can be used to spawn flows.
///
/// First argument represents the enitity itself. It can reference a number of components
/// that are required by the flow.
/// These components will be fetched each time flow is resumed.
/// If non-optional component is missing flow is canceled.
/// Flow declares if it reads or writes into components.
///
/// Second argument is optional and represents the rest of the world.
/// It can be used to access other entities and their components.
pub trait FlowFn {
    fn flow_id() -> TypeId;

    unsafe fn insert(self, world: &mut World, new_flows: &mut dyn AnyNewFlows);
}

/// World pointer that is updated when flow is polled.
/// It can be used to borrow world between awaits.
pub struct FlowWorld {
    view: NonNull<NonNull<World>>,
}

/// Future wrapped to be used as a flow.
struct FutureFlow<F> {
    fut: F,
    world: NonNull<World>,
}

unsafe impl<F> Send for FutureFlow<F> where F: Future + Send {}

impl<F> Flow for FutureFlow<F>
where
    F: Future<Output = ()> + Send + 'static,
{
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>, world: &mut World) -> Poll<()> {
        let this = unsafe { self.get_unchecked_mut() };
        this.world = NonNull::new(world).unwrap();
        let fut = unsafe { Pin::new_unchecked(&mut this.fut) };
        let poll = fut.poll(cx);
        this.world = NonNull::dangling();
        poll
    }
}

impl<F, Fut> FlowFn for F
where
    F: FnOnce(FlowWorld) -> Fut,
    Fut: Future<Output = ()> + 'static,
{
    fn flow_id() -> TypeId {
        TypeId::of::<FutureFlow<Fut>>()
    }

    unsafe fn insert(self, world: &mut World, new_flows: &mut dyn AnyNewFlows) {
        let mut new_flow_task: NewFlowTask<FutureFlow<Fut>> = Arc::new(MaybeUninit::uninit());
        let new_flow_task_mut = Arc::get_mut(&mut new_flow_task).unwrap();

        let flow_ptr =
            addr_of_mut!((*new_flow_task_mut.as_mut_ptr()).flow).cast::<FutureFlow<Fut>>();
        let world_ptr = addr_of_mut!((*flow_ptr).world);
        world_ptr.write(NonNull::from(world));

        let flow_world = FlowWorld {
            view: NonNull::new_unchecked(world_ptr),
        };

        let fut = (self)(flow_world);
        let fut_ptr = addr_of_mut!((*flow_ptr).fut);
        fut_ptr.write(fut);

        let typed_flows = new_flows.downcast_mut::<FutureFlow<Fut>>();

        typed_flows.array.push(new_flow_task);
    }
}
