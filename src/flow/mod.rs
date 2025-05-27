//! Flow module provides API to create high-level async workflows on top of the Edict ECS.
//!
//! A typical use case for flows is to implement game logic where unit moves to a target.
//! An async function can be constructed that causes the unit to move to the target position
//! and resolves when the unit reaches the target or the target is unreachable.
//!
//! # Example
//!
//! ```ignore
//! unit.move_to(target).await
//! ```
//!
//! See the "flow" example for a demonstration.
//!

// There's one possibility to end up with `flow::World` or `flow::Entity` on different thread
// by using scoped threads.
// `flow::World` and `flow::Entity` implement `Send` to allow `Send` bound on the `Future`s which won't be implemented
// if reference to real `World` is kept between awaits that would be unsound.
//
// User-defined auto-trait would be great here to make it instead of `Send` to guard against world reference keeping,
// while making futures `!Send` as well as `flow::World` and `flow::Entity`.
//
// However user-defined auto-traits are far from stable.

use core::{
    any::TypeId,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll, Waker},
};

use alloc::{
    boxed::Box,
    sync::{Arc, Weak},
    task::Wake,
    vec::Vec,
};

use amity::{flip_queue::FlipQueue, ring_buffer::RingBuffer};
use hashbrown::HashMap;
use slab::Slab;

use crate::{
    entity::{EntityId, EntityRef},
    system::State,
    type_id,
    world::{World, WorldLocal},
};

mod entity;
mod futures;
mod tls;
mod world;

pub use self::{entity::*, futures::*, world::*};

/// Task that access world when polled.
pub trait Flow {
    /// Polls the flow.
    ///
    /// # Safety
    ///
    /// Must be called only from flow execution context.
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()>;
}

/// Trait to construct flow instances.
trait MakeFlow: 'static {
    /// Flow type that will be polled.
    type Flow: Flow;

    fn make_flow(self) -> Option<Self::Flow>;
}

/// Returns mutable reference to current world bound to the flow context.
///
/// # Safety
///
/// This may be called only from within flow execution.
/// Caller must ensure that mutable aliasing is not introduced.
/// Which means that another reference returned from [`get_flow_world`] must not exist.
///
/// It is recommended to make sure that reference never escape unsafe block where it is fetched.
#[inline]
pub unsafe fn get_flow_world<'a>() -> &'a mut WorldLocal {
    unsafe { tls::get_world_mut() }
}

/// Type-erased array of newly inserted flows of a single type.
trait AnyMakeFlows {
    /// Returns type of the MakeFlow.
    fn flow_id(&self) -> TypeId;

    /// Drains the array into the queue.
    fn drain(&mut self, flows: &mut HashMap<TypeId, AnyQueue>);
}

impl<'a> dyn AnyMakeFlows + 'a {
    #[inline]
    unsafe fn downcast_mut<F: 'static>(&mut self) -> &mut TypedMakeFlows<F> {
        debug_assert_eq!(self.flow_id(), type_id::<F>());

        unsafe { &mut *(self as *mut Self as *mut TypedMakeFlows<F>) }
    }
}

type FlowMake<F> = <F as MakeFlow>::Flow;

/// Typed array of newly inserted flows of a single type.
struct TypedMakeFlows<F> {
    array: Vec<F>,
}

impl<F> AnyMakeFlows for TypedMakeFlows<F>
where
    F: MakeFlow,
{
    fn flow_id(&self) -> TypeId {
        type_id::<F>()
    }

    fn drain(&mut self, flows: &mut HashMap<TypeId, AnyQueue>) {
        // Short-circuit if there are no new flows.
        if self.array.is_empty() {
            return;
        }

        let flow_id = type_id::<FlowMake<F>>();

        // Find queue for this type of flows or create new one.
        let queue = flows
            .entry(flow_id)
            .or_insert_with(AnyQueue::new::<FlowMake<F>>);

        // Safety: TypedFlows<F> is at index `type_id::<F>()` in `flows.map`.
        let typed_flows = unsafe { queue.flows.downcast_mut::<FlowMake<F>>() };

        // Reserve space to ensure oom can't happen in the loop below.
        typed_flows.array.reserve(self.array.len());

        for make_flow in self.array.drain(..) {
            if let Some(flow) = make_flow.make_flow() {
                let task_id = typed_flows.array.vacant_key();

                let needs_wake = Arc::new(AtomicBool::new(false));
                let task = FlowTask {
                    flow: Box::pin(flow),
                    needs_wake: needs_wake.clone(),
                    waker: Waker::from(Arc::new(FlowWaker {
                        task_id,
                        flip: Arc::downgrade(&queue.flip),
                        needs_wake,
                    })),
                };

                typed_flows.array.insert(task);
                queue.ready.push(task_id);
            }
        }
    }
}

pub(crate) struct NewFlows {
    map: HashMap<TypeId, Box<dyn AnyMakeFlows>>,
}

impl NewFlows {
    pub fn new() -> Self {
        NewFlows {
            map: HashMap::new(),
        }
    }

    fn typed_new_flows<F>(&mut self) -> &mut TypedMakeFlows<F>
    where
        F: MakeFlow,
    {
        let new_flows = self
            .map
            .entry(type_id::<F>())
            .or_insert_with(|| Box::new(TypedMakeFlows::<F> { array: Vec::new() }));

        unsafe { new_flows.downcast_mut::<F>() }
    }

    fn add<F>(&mut self, flow: F)
    where
        F: MakeFlow,
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
    #[inline]
    unsafe fn downcast_mut<F: 'static>(&mut self) -> &mut TypedFlows<F> {
        #[cfg(debug_assertions)]
        assert_eq!(self.flow_id(), type_id::<F>());

        unsafe { &mut *(self as *mut Self as *mut TypedFlows<F>) }
    }
}

struct FlowWaker {
    task_id: usize,
    needs_wake: Arc<AtomicBool>,
    flip: Weak<FlipQueue<usize>>,
}

impl Wake for FlowWaker {
    #[inline]
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    #[inline]
    fn wake_by_ref(self: &Arc<Self>) {
        let needs_wake = self.needs_wake.fetch_and(false, Ordering::Acquire);
        if !needs_wake {
            return;
        }
        let Some(flip) = self.flip.upgrade() else {
            return;
        };
        flip.push_sync(self.task_id);
    }
}

struct FlowTask<F> {
    flow: Pin<Box<F>>,
    needs_wake: Arc<AtomicBool>,
    waker: Waker,
}

/// Container of spawned flows of specific type.
struct TypedFlows<F> {
    array: Slab<FlowTask<F>>,
}

impl<F> TypedFlows<F>
where
    F: Flow + 'static,
{
    #[inline]
    unsafe fn execute(&mut self, ids: &[usize]) {
        for &id in ids {
            let Some(task) = self.array.get_mut(id) else {
                continue;
            };

            task.needs_wake.store(true, Ordering::Relaxed);

            let mut cx = Context::from_waker(&task.waker);

            // Safety: This is the only code that can access `task.flow`.
            // It is destroyed in-place when it is ready or TypedFlows is dropped.

            let pinned = task.flow.as_mut();

            // This is the only safe place to poll the flow.
            let poll = unsafe { F::poll(pinned, &mut cx) };

            if let Poll::Ready(()) = poll {
                self.array.remove(id);
            }
        }
    }
}

impl<F> AnyFlows for TypedFlows<F>
where
    F: Flow + 'static,
{
    #[cfg(debug_assertions)]
    fn flow_id(&self) -> TypeId {
        type_id::<F>()
    }

    unsafe fn execute(&mut self, front: &[usize], back: &[usize]) {
        unsafe {
            self.execute(front);
        }
        unsafe {
            self.execute(back);
        }
    }
}

/// Queue of flows of a single type.
struct AnyQueue {
    flip: Arc<FlipQueue<usize>>,
    ready: RingBuffer<usize>,
    flows: Box<dyn AnyFlows>,
}

impl AnyQueue {
    fn new<F>() -> Self
    where
        F: Flow + 'static,
    {
        AnyQueue {
            flip: Arc::new(FlipQueue::new()),
            ready: RingBuffer::new(),
            flows: Box::new(TypedFlows::<F> { array: Slab::new() }),
        }
    }
}

/// Flows container manages running flows,
/// collects spawned flows and executes them.
pub struct Flows {
    new_flows: NewFlows,
    map: HashMap<TypeId, AnyQueue>,
}

impl Default for Flows {
    fn default() -> Self {
        Self::new()
    }
}

impl Flows {
    /// Creates a new instance of `Flows`.
    ///
    /// There should be instance of `Flows` for each `World` to execute flows spawned in the world.
    /// One `Flows` should be used with single `World` instance.
    pub fn new() -> Self {
        Flows {
            new_flows: NewFlows::new(),
            map: HashMap::new(),
        }
    }

    fn collect_new_flows<'a>(&mut self, world: &'a mut World) -> Option<tls::WorldGuard<'a>> {
        let world = world.local();

        core::mem::swap(&mut self.new_flows, world.new_flows.get_mut());

        let guard = tls::WorldGuard::new(world);

        // First swap all queues with ready buffer.
        for typed in self.map.values_mut() {
            debug_assert!(typed.ready.is_empty());
            typed.flip.swap_buffer(&mut typed.ready);
        }

        // Then drain all new flows into queues.
        // New flow ids are added to ready buffer.
        for (_, typed) in &mut self.new_flows.map {
            typed.drain(&mut self.map);
        }

        Some(guard)
    }

    /// Executes all ready flows in the world.
    ///
    /// Flows spawned in the world are drained into this instance,
    /// so this function should be called with the same world instance.
    pub fn execute(&mut self, world: &mut World) {
        world.maintenance();

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

    /// Enter flow context and execute the closure with the [`FlowWorld`] instance.
    ///
    /// Closure may use any [`FlowWorld`] and [`FlowEntity`] values.
    pub fn enter<F, R>(world: &mut World, f: F) -> R
    where
        F: FnOnce(FlowWorld) -> R,
    {
        let guard = tls::WorldGuard::new(world.local());
        let r = f(FlowWorld::new());
        drop(guard);
        r
    }
}

/// Function that can be used as a [`System`](crate::system::System)
/// to execute flows in the ECS world.
pub fn flows_system(world: &mut World, mut flows: State<Flows>) {
    let flows = &mut *flows;
    flows.execute(world);
}

struct EntityIntoFlow<F> {
    entity: EntityId,
    f: F,
}

impl<F> MakeFlow for EntityIntoFlow<F>
where
    F: IntoEntityFlow,
{
    type Flow = F::Flow;

    fn make_flow(self) -> Option<F::Flow> {
        let e = FlowEntity::new(self.entity);

        if e.is_alive() {
            unsafe { self.f.into_entity_flow(e) }
        } else {
            None
        }
    }
}

struct WorldIntoFlow<F> {
    f: F,
}

impl<F> MakeFlow for WorldIntoFlow<F>
where
    F: IntoFlow,
{
    type Flow = F::Flow;

    fn make_flow(self) -> Option<F::Flow> {
        self.f.into_flow(FlowWorld::new())
    }
}

impl World {
    /// Spawns a flow in the world.
    /// It will be polled during [`Flows::execute`] until completion.
    pub fn spawn_flow<F>(&mut self, flow: F)
    where
        F: IntoFlow,
    {
        self.new_flows.get_mut().add(WorldIntoFlow { f: flow });
    }

    /// Spawns a flow for an entity in the world.
    /// It will be polled during [`Flows::execute`] until completion
    /// or until the entity is despawned.
    pub fn spawn_flow_for<F>(&mut self, entity: EntityId, flow: F)
    where
        F: IntoEntityFlow,
    {
        self.new_flows
            .get_mut()
            .add(EntityIntoFlow { entity, f: flow });
    }
}

impl WorldLocal {
    /// Spawn a flow in the world.
    /// It will be polled during [`Flows::execute`] until completion.
    pub fn spawn_flow<F>(&self, flow: F)
    where
        F: IntoFlow,
    {
        // Safety: accessed only from "main" thread.
        unsafe { &mut *self.new_flows.get() }.add(WorldIntoFlow { f: flow });
    }

    /// Spawns a flow for an entity in the world.
    /// It will be polled during [`Flows::execute`] until completion
    /// or until the entity is despawned.
    pub fn spawn_flow_for<F>(&self, entity: EntityId, flow: F)
    where
        F: IntoEntityFlow,
    {
        // Safety: accessed only from "main" thread.
        unsafe { &mut *self.new_flows.get() }.add(EntityIntoFlow { entity, f: flow });
    }
}

impl FlowWorld {
    /// Spawn a flow in the world.
    /// It will be polled during [`Flows::execute`] until completion.
    pub fn spawn_flow<F>(self, flow: F)
    where
        F: IntoFlow,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.spawn_flow(flow);
    }

    /// Spawns a flow for an entity in the world.
    /// It will be polled during [`Flows::execute`] until completion
    /// or until the entity is despawned.
    pub fn spawn_flow_for<F>(&self, entity: EntityId, flow: F)
    where
        F: IntoEntityFlow,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.spawn_flow_for(entity, flow);
    }
}

impl EntityRef<'_> {
    /// Spawns a new flow for the entity.
    pub fn spawn_flow<F>(&mut self, f: F)
    where
        F: crate::flow::IntoEntityFlow,
    {
        let id = self.id();
        self.world().spawn_flow_for(id, f);
    }
}
