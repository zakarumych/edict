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
//! ```ignore
//! unit.move_to(target).await
//! ```
//!

use core::{
    any::TypeId,
    cell::UnsafeCell,
    future::Future,
    mem::{ManuallyDrop, MaybeUninit},
    pin::Pin,
    ptr::{addr_of, addr_of_mut},
    task::{Context, Poll, Waker},
};

use alloc::task::Wake;

use alloc::sync::Arc;
use amity::{flip_queue::FlipQueue, ring_buffer::RingBuffer};
use hashbrown::HashMap;
use slab::Slab;

use crate::{system::State, tls, world::World};

mod entity;
mod world;

pub use self::{entity::*, world::*};

/// Task that access world when polled.
pub trait Flow: Send + 'static {
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()>;
}

/// Call only from flow context.
///
/// This is public for use by custom flows.
/// Built-in flows use it internally from `FlowWorld` and `FlowEntity`.
#[inline(always)]
pub unsafe fn flow_world<'a>() -> &'a mut World {
    unsafe { tls::get_world() }
}

/// Type-erased array of newly inserted flows of a single type.
trait AnyNewFlows: Send {
    /// Returns type of the flow.
    #[cfg(debug_assertions)]
    fn flow_id(&self) -> TypeId;

    /// Drains the array into the queue.
    fn drain(&mut self, flows: &mut Flows);
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

    fn drain(&mut self, flows: &mut Flows) {
        // Short-circuit if there are no new flows.
        if self.array.is_empty() {
            return;
        }

        // Find queue for this type of flows or create new one.
        let queue = flows
            .map
            .entry(TypeId::of::<F>())
            .or_insert_with(|| AnyQueue {
                queue: Arc::new(FlipQueue::new()),
                ready: RingBuffer::new(),
                flows: Box::new(TypedFlows::<F> { array: Slab::new() }),
            });

        // Safety: TypedFlows<F> is at index `TypeId::of::<F>()` in `flows.map`.
        let typed_flows = unsafe { queue.flows.downcast_mut::<F>() };

        // Reserve space to ensure oom can't happen in the loop below.
        typed_flows.array.reserve(self.array.len());

        for mut task in self.array.drain(..) {
            let id = typed_flows.array.vacant_key();

            // Finish construction of the FlowTask value.
            let task = unsafe {
                // Safety: `task` is still unique.
                let flow_mut = Arc::get_mut(&mut task).unwrap_unchecked();
                addr_of_mut!((*flow_mut.as_mut_ptr()).id).write(id);
                addr_of_mut!((*flow_mut.as_mut_ptr()).queue).write(queue.queue.clone());

                // All three fields of the FlowTask are initialized. Cast to MaybeUninit away.
                // This is valid because Arc does not use niche optimizations and so layout is the same.
                Arc::from_raw(Arc::<MaybeUninit<FlowTask<F>>>::into_raw(task) as *const FlowTask<F>)
            };

            typed_flows.array.insert(task);
            queue.ready.push(id);
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
    pub fn typed_new_flows<F>(&mut self) -> &mut TypedNewFlows<F>
    where
        F: Flow,
    {
        let new_flows = self
            .map
            .entry(TypeId::of::<F>())
            .or_insert_with(|| Box::new(TypedNewFlows::<F> { array: Vec::new() }));

        unsafe { new_flows.downcast_mut::<F>() }
    }

    pub fn add<F>(&mut self, flow: F)
    where
        F: Flow,
    {
        let typed_new_flows = self.typed_new_flows();

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

    unsafe fn execute(&mut self, front: &[usize], back: &[usize]);
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

impl<F> TypedFlows<F>
where
    F: Flow,
{
    #[inline]
    unsafe fn execute(&mut self, ids: &[usize]) {
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
                unsafe { pinned.poll(&mut cx) }
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

impl<F> AnyFlows for TypedFlows<F>
where
    F: Flow,
{
    #[cfg(debug_assertions)]
    fn flow_id(&self) -> TypeId {
        TypeId::of::<F>()
    }

    unsafe fn execute(&mut self, front: &[usize], back: &[usize]) {
        self.execute(front);
        self.execute(back);
    }
}

/// Queue of flows of a single type.
struct AnyQueue {
    queue: Arc<FlipQueue<usize>>,
    ready: RingBuffer<usize>,
    flows: Box<dyn AnyFlows>,
}

/// Queues of all types of flows.
pub struct Flows {
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

    {
        let mut new_flows = match world.get_resource_mut::<NewFlows>() {
            None => return,
            Some(new_flows) => new_flows,
        };

        // First swap all queues with ready buffer.
        for typed in flows.map.values_mut() {
            debug_assert!(typed.ready.is_empty());
            typed.queue.swap_buffer(&mut typed.ready);
        }

        // Then drain all new flows into queues.
        // New flow ids are added to ready buffer.
        for (_, typed) in &mut new_flows.map {
            typed.drain(flows);
        }
    }

    let _guard = tls::Guard::new(world);

    // Execute all ready flows.
    for typed in flows.map.values_mut() {
        let (front, back) = typed.ready.as_slices();
        unsafe {
            typed.flows.execute(front, back);
        }

        // Clear ready buffer.
        typed.ready.clear();
    }
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

/// Spawns code block as a flow.
#[macro_export]
macro_rules! spawn_block {
    (in $world:ident -> $($closure:tt)*) => {
        $crate::flow::spawn(&mut $world, $crate::flow_closure!(|mut $world| { $($closure)* }));
    };
    (in ref $world:ident -> $($closure:tt)*) => {
        $crate::flow::spawn($world, $crate::flow_closure!(|mut $world| { $($closure)* }));
    };
    (in $world:ident for $entity:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_for($entity, &mut $world, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    };
    (in ref $world:ident for $entity:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_for($entity, $world, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    };
    (for $entity:ident in $world:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_for($entity, &mut $world, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    };
    (for $entity:ident in ref $world:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_for($entity, $world, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    };
}

pub struct YieldNow {
    yielded: bool,
}

impl YieldNow {
    pub fn new() -> Self {
        YieldNow { yielded: false }
    }
}

impl Future for YieldNow {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.get_mut();
        if !me.yielded {
            me.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

#[macro_export]
macro_rules! yield_now {
    () => {
        $crate::private::YieldNow::new().await
    };
}
