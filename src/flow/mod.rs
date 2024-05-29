//! Flow module provides API to create high-level async workflows on top of the Edict ECS.
//!
//! Typical example would be a flow that causes an entity to move towards a target.
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
    mem::ManuallyDrop,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use alloc::task::Wake;

use alloc::sync::Arc;
use amity::{flip_queue::FlipQueue, ring_buffer::RingBuffer};
use hashbrown::HashMap;
use slab::Slab;

use crate::{
    system::State,
    tls, type_id,
    world::{World, WorldLocal},
    EntityId,
};

mod entity;
mod world;

pub use self::{entity::*, world::*};

/// Task that access world when polled.
pub trait Flow: 'static {
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()>;
}

pub trait IntoFlow: 'static {
    type Flow: Flow;

    /// Converts flow into the inner flow type.
    unsafe fn into_flow(self) -> Self::Flow;
}

/// Call only from flow context.
///
/// This is public for use by custom flows.
/// Built-in flows use it internally from `FlowWorld` and `FlowEntity`.
#[inline(always)]
pub unsafe fn flow_world_ref<'a>() -> &'a WorldLocal {
    unsafe { tls::get_world_ref() }
}

/// Call only from flow context.
///
/// This is public for use by custom flows.
/// Built-in flows use it internally from `FlowWorld` and `FlowEntity`.
#[inline(always)]
pub unsafe fn flow_world_mut<'a>() -> &'a mut WorldLocal {
    unsafe { tls::get_world_mut() }
}

/// Type-erased array of newly inserted flows of a single type.
trait AnyIntoFlows {
    /// Returns type of the IntoFlow.
    fn flow_id(&self) -> TypeId;

    /// Drains the array into the queue.
    fn drain(&mut self, flows: &mut HashMap<TypeId, AnyQueue>);
}

impl<'a> dyn AnyIntoFlows + 'a {
    #[inline(always)]
    unsafe fn downcast_mut<F: 'static>(&mut self) -> &mut TypedIntoFlows<F> {
        debug_assert_eq!(self.flow_id(), type_id::<F>());

        unsafe { &mut *(self as *mut Self as *mut TypedIntoFlows<F>) }
    }
}

impl<'a> dyn AnyIntoFlows + Send + 'a {
    #[inline(always)]
    unsafe fn downcast_mut<F: 'static>(&mut self) -> &mut TypedIntoFlows<F> {
        debug_assert_eq!(self.flow_id(), type_id::<F>());

        unsafe { &mut *(self as *mut Self as *mut TypedIntoFlows<F>) }
    }
}

/// Typed array of newly inserted flows of a single type.
struct TypedIntoFlows<F> {
    array: Vec<F>,
}

impl<F> AnyIntoFlows for TypedIntoFlows<F>
where
    F: IntoFlow,
{
    fn flow_id(&self) -> TypeId {
        type_id::<F>()
    }

    fn drain(&mut self, flows: &mut HashMap<TypeId, AnyQueue>) {
        // Short-circuit if there are no new flows.
        if self.array.is_empty() {
            return;
        }

        let flow_id = type_id::<F::Flow>();

        // Find queue for this type of flows or create new one.
        let queue = flows
            .entry(flow_id)
            .or_insert_with(AnyQueue::new::<F::Flow>);

        // Safety: TypedFlows<F> is at index `type_id::<F>()` in `flows.map`.
        let typed_flows = unsafe { queue.flows.downcast_mut::<F::Flow>() };

        // Reserve space to ensure oom can't happen in the loop below.
        typed_flows.array.reserve(self.array.len());

        for into_flow in self.array.drain(..) {
            let id = typed_flows.array.vacant_key();

            let task = FlowTask {
                flow: UnsafeCell::new(ManuallyDrop::new(unsafe { into_flow.into_flow() })),
                id,
                queue: queue.queue.clone(),
            };

            typed_flows.array.insert(Arc::new(task));
            queue.ready.push(id);
        }
    }
}

struct NewSendFlows {
    map: HashMap<TypeId, Box<dyn AnyIntoFlows + Send>>,
}

impl Default for NewSendFlows {
    fn default() -> Self {
        Self::new()
    }
}

impl NewSendFlows {
    fn new() -> Self {
        NewSendFlows {
            map: HashMap::new(),
        }
    }

    pub fn typed_new_flows<F>(&mut self) -> &mut TypedIntoFlows<F>
    where
        F: IntoFlow + Send,
    {
        let new_flows = self
            .map
            .entry(type_id::<F>())
            .or_insert_with(|| Box::new(TypedIntoFlows::<F> { array: Vec::new() }));

        unsafe { new_flows.downcast_mut::<F>() }
    }

    pub fn add<F>(&mut self, flow: F)
    where
        F: IntoFlow + Send,
    {
        let typed_new_flows = self.typed_new_flows();
        typed_new_flows.array.push(flow);
    }
}

struct NewLocalFlows {
    map: HashMap<TypeId, Box<dyn AnyIntoFlows>>,
}

impl Default for NewLocalFlows {
    fn default() -> Self {
        Self::new()
    }
}

impl NewLocalFlows {
    fn new() -> Self {
        NewLocalFlows {
            map: HashMap::new(),
        }
    }

    pub fn typed_new_flows<F>(&mut self) -> &mut TypedIntoFlows<F>
    where
        F: IntoFlow,
    {
        let new_flows = self
            .map
            .entry(type_id::<F>())
            .or_insert_with(|| Box::new(TypedIntoFlows::<F> { array: Vec::new() }));

        unsafe { new_flows.downcast_mut::<F>() }
    }

    pub fn add<F>(&mut self, flow: F)
    where
        F: IntoFlow,
    {
        let typed_new_flows = self.typed_new_flows();
        typed_new_flows.array.push(flow);
    }
}

/// Trait implemented by `TypedFlows` with `F: Flow`
trait AnyFlows {
    #[cfg(debug_assertions)]
    fn flow_id(&self) -> TypeId;

    unsafe fn execute(&mut self, front: &[usize], back: &[usize]);
}

impl dyn AnyFlows {
    #[inline(always)]
    unsafe fn downcast_mut<F: 'static>(&mut self) -> &mut TypedFlows<F> {
        #[cfg(debug_assertions)]
        assert_eq!(self.flow_id(), type_id::<F>());

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

impl<F> Wake for FlowTask<F>
where
    F: Flow,
{
    fn wake(self: Arc<Self>) {
        self.queue.push(self.id);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.push(self.id);
    }
}

impl<F> FlowTask<F>
where
    F: Flow,
{
    fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }
}

/// Container of spawned flows of specific type.
struct TypedFlows<F> {
    array: Slab<Arc<FlowTask<F>>>,
}

impl<F> TypedFlows<F>
where
    F: Flow,
{
    #[inline(always)]
    unsafe fn execute(&mut self, ids: &[usize]) {
        for &id in ids {
            let Some(task) = self.array.get(id) else {
                continue;
            };

            let waker = task.waker();
            let mut cx = Context::from_waker(&waker);

            // Safety: This is the only code that can access `task.flow`.
            // It is destroyed in-place when it is ready or TypedFlows is dropped.
            let poll = unsafe {
                let pinned = Pin::new_unchecked(&mut **task.flow.get());
                unsafe { pinned.poll(&mut cx) }
            };

            if let Poll::Ready(()) = poll {
                let task = self.array.remove(id);
                // Safety: Removed from array. `task.flow` is inaccessible anywhere but here.
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
        type_id::<F>()
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

impl AnyQueue {
    fn new<F: Flow>() -> Self {
        AnyQueue {
            queue: Arc::new(FlipQueue::new()),
            ready: RingBuffer::new(),
            flows: Box::new(TypedFlows::<F> { array: Slab::new() }),
        }
    }
}

/// Flows container manages running flows,
/// collects spawned flows and executes them.
pub struct Flows {
    new_flows: NewSendFlows,
    new_local_flows: NewLocalFlows,
    map: HashMap<TypeId, AnyQueue>,
}

impl Default for Flows {
    fn default() -> Self {
        Self::new()
    }
}

impl Flows {
    pub fn new() -> Self {
        Flows {
            new_flows: NewSendFlows::new(),
            new_local_flows: NewLocalFlows::new(),
            map: HashMap::new(),
        }
    }

    /// Call at least once prior to spawning flows.
    pub fn init(world: &mut World) {
        world.with_resource(NewSendFlows::new);
        world.with_resource(NewLocalFlows::new);
    }

    fn collect_new_flows<'a>(&mut self, world: &'a mut World) -> Option<tls::Guard<'a>> {
        let world = world.local();

        let mut new_flows_res = match world.get_resource_mut::<NewSendFlows>() {
            None => return None,
            Some(new_flows) => new_flows,
        };

        std::mem::swap(&mut self.new_flows, &mut *new_flows_res);
        drop(new_flows_res);

        let mut new_local_flows_res = match world.get_resource_mut::<NewLocalFlows>() {
            None => return None,
            Some(new_local_flows) => new_local_flows,
        };

        std::mem::swap(&mut self.new_local_flows, &mut *new_local_flows_res);
        drop(new_local_flows_res);

        let guard = tls::Guard::new(world);

        // First swap all queues with ready buffer.
        for typed in self.map.values_mut() {
            debug_assert!(typed.ready.is_empty());
            typed.queue.swap_buffer(&mut typed.ready);
        }

        // Then drain all new flows into queues.
        // New flow ids are added to ready buffer.
        for (_, typed) in &mut self.new_flows.map {
            typed.drain(&mut self.map);
        }
        for (_, typed) in &mut self.new_local_flows.map {
            typed.drain(&mut self.map);
        }

        Some(guard)
    }

    pub fn execute(&mut self, world: &mut World) {
        let Some(_guard) = self.collect_new_flows(world) else {
            return;
        };

        // Execute all ready flows.
        for typed in self.map.values_mut() {
            let (front, back) = typed.ready.as_slices();
            unsafe {
                typed.flows.execute(front, back);
            }

            // Clear ready buffer.
            typed.ready.clear();
        }
    }
}

/// System that executes flows spawned in the world.
pub fn flows_system(world: &mut World, mut flows: State<Flows>) {
    let flows = &mut *flows;
    flows.execute(world);
}

/// Spawn a flow into the world.
///
/// The flow will be polled by the `flows_system`.
pub fn spawn<F>(world: &World, flow: F)
where
    F: IntoFlow + Send,
{
    world.expect_resource_mut::<NewSendFlows>().add(flow);
}

/// Spawn a flow into the world.
///
/// The flow will be polled by the `flows_system`.
pub fn spawn_local<F>(world: &WorldLocal, flow: F)
where
    F: IntoFlow,
{
    world.expect_resource_mut::<NewLocalFlows>().add(flow);
}

/// Spawn a flow for the entity into the world.
///
/// The flow will be polled by the `flows_system`.
pub fn spawn_for<F>(world: &World, id: EntityId, flow: F)
where
    F: IntoEntityFlow + Send,
{
    struct AdHoc<F> {
        id: EntityId,
        f: F,
    }

    impl<F> IntoFlow for AdHoc<F>
    where
        F: IntoEntityFlow,
    {
        type Flow = F::Flow;

        unsafe fn into_flow(self) -> F::Flow {
            unsafe { self.f.into_entity_flow(self.id) }
        }
    }

    spawn(world, AdHoc { id, f: flow });
}

/// Spawn a flow for the entity into the world.
///
/// The flow will be polled by the `flows_system`.
pub fn spawn_local_for<F>(world: &WorldLocal, id: EntityId, flow: F)
where
    F: IntoEntityFlow,
{
    struct AdHoc<F> {
        id: EntityId,
        f: F,
    }

    impl<F> IntoFlow for AdHoc<F>
    where
        F: IntoEntityFlow,
    {
        type Flow = F::Flow;

        unsafe fn into_flow(self) -> F::Flow {
            unsafe { self.f.into_entity_flow(self.id) }
        }
    }

    spawn_local(world, AdHoc { id, f: flow });
}

/// Spawns code block as a flow.
#[macro_export]
macro_rules! spawn_block {
    (in $world:ident -> $($closure:tt)*) => {
        $crate::flow::spawn(&$world, $crate::flow_closure!(|mut $world| { $($closure)* }));
    };
    (in $world:ident for $entity:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_for(&$world, $entity, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    };
    (for $entity:ident in $world:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_for(&$world, $entity, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    };
    (local $world:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_local(&$world, $crate::flow_closure!(|mut $world| { $($closure)* }));
    };
    (local $world:ident for $entity:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_local_for(&$world, $entity, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    };
    (for $entity:ident local $world:ident -> $($closure:tt)*) => {
        $crate::flow::spawn_local_for(&$world, $entity, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    };
    (for $entity:ident -> $($closure:tt)*) => {{
        let e = $entity.id();
        let w = $entity.get_world();
        $crate::flow::spawn_local_for(w, e, $crate::flow_closure_for!(|mut $entity| { $($closure)* }));
    }};
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
