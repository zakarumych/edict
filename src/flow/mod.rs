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
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use alloc::{boxed::Box, sync::Arc, task::Wake, vec::Vec};

use amity::{flip_queue::FlipQueue, ring_buffer::RingBuffer};
use hashbrown::HashMap;
use slab::Slab;

use crate::{entity::EntityId, system::State, type_id, world::WorldLocal};

mod entity;
mod futures;
mod tls;
mod world;

pub use edict_proc::flow_fn;

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

/// Type that can be spawned as a flow.
/// It can be an async function or a closure inside `flow_fn!` macro.
/// It must accept [`&mut World`](World) as the only argument.
///
/// # Example
///
/// ```
/// # use edict::flow::{self, flow_fn, World};
///
/// let mut world = edict::world::World::new();
///
/// world.spawn_flow(flow_fn!(|world: &mut World| {
///   let entity = world.spawn(());
/// }));
/// ```
#[diagnostic::on_unimplemented(
    note = "Try `async fn(world: &mut flow::World)` or `flow_fn!(|world: &mut flow::World| {{ ... }})`"
)]
pub trait IntoFlow: 'static {
    /// Flow type that will be polled.
    type Flow<'a>: Flow + 'a;

    /// Converts self into a flow.
    fn into_flow<'a>(self, world: &'a mut World) -> Option<Self::Flow<'a>>;
}

/// Call only from flow context when mutable references to world do not exist.
///
/// This is public for use by custom flows.
/// Built-in flows use it internally from `World` and `Entity`.
#[inline(always)]
pub unsafe fn flow_world_ref<'a>() -> &'a WorldLocal {
    unsafe { tls::get_world_ref() }
}

/// Call only from flow context when other references to world do not exist.
///
/// This is public for use by custom flows.
/// Built-in flows use it internally from `World` and `Entity`.
#[inline(always)]
pub unsafe fn flow_world_mut<'a>() -> &'a mut WorldLocal {
    unsafe { tls::get_world_mut() }
}

/// Returns current flow's entity id.
/// If called outside entity flow poll it returns `None`.
#[inline(always)]
pub fn flow_entity() -> Option<EntityId> {
    tls::get_entity()
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

type FlowInto<F> = <F as IntoFlow>::Flow<'static>;

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

        let flow_id = type_id::<FlowInto<F>>();

        // Find queue for this type of flows or create new one.
        let queue = flows
            .entry(flow_id)
            .or_insert_with(AnyQueue::new::<FlowInto<F>>);

        // Safety: TypedFlows<F> is at index `type_id::<F>()` in `flows.map`.
        let typed_flows = unsafe { queue.flows.downcast_mut::<FlowInto<F>>() };

        // Reserve space to ensure oom can't happen in the loop below.
        typed_flows.array.reserve(self.array.len());

        for into_flow in self.array.drain(..) {
            if let Some(flow) = unsafe { into_flow.into_flow(World::make_mut()) } {
                let task_id = typed_flows.array.vacant_key();

                let task = FlowTask {
                    flow: Box::pin(flow),
                    waker: Waker::from(Arc::new(FlowWaker {
                        task_id,
                        queue: queue.queue.clone(),
                    })),
                };

                typed_flows.array.insert(task);
                queue.ready.push(task_id);
            }
        }
    }
}

pub(crate) struct NewFlows {
    map: HashMap<TypeId, Box<dyn AnyIntoFlows>>,
}

impl NewFlows {
    pub fn new() -> Self {
        NewFlows {
            map: HashMap::new(),
        }
    }

    fn typed_new_flows<F>(&mut self) -> &mut TypedIntoFlows<F>
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

struct FlowWaker {
    task_id: usize,
    queue: Arc<FlipQueue<usize>>,
}

impl Wake for FlowWaker {
    fn wake(self: Arc<Self>) {
        self.queue.push(self.task_id);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.push(self.task_id);
    }
}

struct FlowTask<F> {
    flow: Pin<Box<F>>,
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
    #[inline(always)]
    unsafe fn execute(&mut self, ids: &[usize]) {
        for &id in ids {
            let Some(task) = self.array.get_mut(id) else {
                continue;
            };

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
    queue: Arc<FlipQueue<usize>>,
    ready: RingBuffer<usize>,
    flows: Box<dyn AnyFlows>,
}

impl AnyQueue {
    fn new<F>() -> Self
    where
        F: Flow + 'static,
    {
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

    fn collect_new_flows<'a>(
        &mut self,
        world: &'a mut crate::world::World,
    ) -> Option<tls::WorldGuard<'a>> {
        let world = world.local();

        core::mem::swap(&mut self.new_flows, world.new_flows.get_mut());

        let guard = tls::WorldGuard::new(world);

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

        Some(guard)
    }

    /// Executes all ready flows in the world.
    ///
    /// Flows spawned in the world are drained into this instance,
    /// so this function should be called with the same world instance.
    pub fn execute(&mut self, world: &mut crate::world::World) {
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
}

/// Function that can be used as a [`System`](crate::system::System)
/// to execute flows in the ECS world.
pub fn flows_system(world: &mut crate::world::World, mut flows: State<Flows>) {
    let flows = &mut *flows;
    flows.execute(world);
}

struct EntityIntoFlow<F> {
    entity: EntityId,
    f: F,
}

impl<F> IntoFlow for EntityIntoFlow<F>
where
    F: IntoEntityFlow,
{
    type Flow<'a> = F::Flow<'a>;

    fn into_flow<'a>(self, world: &'a mut World) -> Option<F::Flow<'a>> {
        let e = world.entity(self.entity).ok()?;

        unsafe { self.f.into_entity_flow(e) }
    }
}

impl crate::world::World {
    /// Spawns a flow in the world.
    /// It will be polled during [`Flows::execute`] until completion.
    pub fn spawn_flow<F>(&mut self, flow: F)
    where
        F: IntoFlow,
    {
        self.new_flows.get_mut().add(flow);
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
        unsafe { &mut *self.new_flows.get() }.add(flow);
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

#[doc(hidden)]
pub struct FlowClosure<F, Fut> {
    f: F,
    _phantom: PhantomData<Fut>,
}

impl<F, Fut> FlowClosure<F, Fut> {
    pub fn new<C>(f: F) -> Self
    where
        F: FnOnce(C) -> Fut + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        FlowClosure {
            f,
            _phantom: PhantomData,
        }
    }
}

#[diagnostic::on_unimplemented(
    message = "Expected `&mut flow::World` or `Entity<'_>`",
    label = "Closure inside `flow_fn!` macro must have `&mut flow::World` or `Entity<'_>` argument",
    note = "When `flow_fn!` is used with `World::spawn_flow` closure must have `&mut flow::World` argument",
    note = "When `flow_fn!` is used with `Entity::spawn_flow` closure must have `Entity<'_>` argument"
)]
#[doc(hidden)]
pub trait FlowContext<'a> {
    type Token;

    fn cx(token: &'a Self::Token) -> Self;
}

// Changed into proc-macro
// /// Converts closure syntax to flow fn.
// ///
// /// There's limitation that makes following `|world: World<'_>| async move { /*use world*/ }`
// /// to be non-compilable.
// /// Currently it requires `async |world: World<'_>| { /*use world*/ }` syntax which is not stable.
// ///
// /// On nightly it is possible to use `async move |world: World<'_>| { /*use world*/ }`
// /// But this syntax is not stable yet and edict avoids requiring too many nighty features.
// ///
// /// This macro is a workaround for this limitation.
// #[macro_export]
// macro_rules! flow_fn {
//     (|$arg:ident $(: $ty:ty)?| $code:expr) => {
//         unsafe {
//             $crate::flow::FlowClosure::new(move |cx| async move {
//                 #[allow(unused)]
//                 let $arg $(: $ty)? = $crate::flow::FlowContext::cx(&cx);
//                 {
//                     $code
//                 }
//             })
//         }
//     };
//     (|mut $arg:ident $(: $ty:ty)?| $code:expr) => {
//         unsafe {
//             $crate::flow::FlowClosure::new(move |cx| async move {
//                 #[allow(unused)]
//                 let mut $arg $(: $ty)? = $crate::flow::FlowContext::cx(&cx);
//                 {
//                     $code
//                 }
//             })
//         }
//     };
// }
