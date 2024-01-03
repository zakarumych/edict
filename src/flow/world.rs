use core::{
    future::Future,
    marker::PhantomData,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    pin::Pin,
    ptr::addr_of_mut,
    task::{Context, Poll},
};

use alloc::sync::Arc;

use crate::world::World;

use super::{flow_world, Flow, NewFlowTask, NewFlows};

/// World reference that is updated when flow is polled.
pub struct FlowWorld<'a> {
    pub(super) marker: PhantomData<&'a mut World>,
}

unsafe impl Send for FlowWorld<'_> {}
unsafe impl Sync for FlowWorld<'_> {}

impl Deref for FlowWorld<'_> {
    type Target = World;

    fn deref(&self) -> &Self::Target {
        unsafe { flow_world() }
    }
}

impl DerefMut for FlowWorld<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { flow_world() }
    }
}

/// Future wrapped to be used as a flow.
#[repr(transparent)]
struct FutureFlow<F> {
    fut: F,
}

impl<F> Flow for FutureFlow<F>
where
    F: Future<Output = ()> + Send + 'static,
{
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = unsafe { self.get_unchecked_mut() };
        let fut = unsafe { Pin::new_unchecked(&mut this.fut) };
        let poll = fut.poll(cx);
        poll
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
pub trait WorldFlowFn<'a> {
    type Fut: Future<Output = ()> + Send + 'a;
    fn run(self, world: FlowWorld<'a>) -> Self::Fut;
}

#[doc(hidden)]
pub fn insert_world_flow<F>(world: &mut World, f: F)
where
    F: WorldFlowFn<'static>,
{
    let mut new_flow_task: NewFlowTask<FutureFlow<F::Fut>> = Arc::new(MaybeUninit::uninit());
    let new_flow_task_mut = Arc::get_mut(&mut new_flow_task).unwrap();

    unsafe {
        let flow_ptr =
            addr_of_mut!((*new_flow_task_mut.as_mut_ptr()).flow).cast::<FutureFlow<F::Fut>>();

        let fut = f.run(FlowWorld::new());
        let fut_ptr = addr_of_mut!((*flow_ptr).fut);
        fut_ptr.write(fut);
    }

    world
        .with_default_resource::<NewFlows>()
        .typed_new_flows()
        .array
        .push(new_flow_task);
}

// Must be callable with any lifetime of `FlowWorld` borrow.
impl<'a, F, Fut> WorldFlowFn<'a> for F
where
    F: FnOnce(FlowWorld<'a>) -> Fut,
    Fut: Future<Output = ()> + Send + 'a,
{
    type Fut = Fut;

    fn run(self, world: FlowWorld<'a>) -> Fut {
        self(world)
    }
}

pub trait WorldFlowSpawn {
    fn spawn(self, world: &mut World);
}

impl<F> WorldFlowSpawn for F
where
    F: for<'a> WorldFlowFn<'a>,
{
    fn spawn(self, world: &mut World) {
        insert_world_flow(world, self);
    }
}

pub struct WorldClosureSpawn<F>(pub F);

impl<F> WorldFlowSpawn for WorldClosureSpawn<F>
where
    F: FnOnce(&mut World),
{
    fn spawn(self, world: &mut World) {
        (self.0)(world)
    }
}

/// Converts closure syntax to flow fn.
///
/// There's limitation that makes following `|world: FlowWorld<'_>| async move { /*use world*/ }`
/// to be noncompilable.
///
/// On nightly it is possible to use `async move |world: FlowWorld<'_>| { /*use world*/ }`
/// But this syntax is not stable yet and edict avoids requiring too many nighty features.
///
/// This macro is a workaround for this limitation.
#[macro_export]
macro_rules! flow_closure {
    (|mut $world:ident $(: $FlowWorld:ty)?| -> $ret:ty $code:block) => {
        $crate::private::WorldClosureSpawn(|world: &mut $crate::world::World| {
            $crate::private::insert_world_flow(
                world,
                |mut world: $crate::flow::FlowWorld<'static>| async move {
                    #[allow(unused_mut)]
                    let mut $world $(: $FlowWorld)? = world.reborrow();
                    let res: $ret = { $code };
                    res
                },
            )
        })
    };
    (|mut $world:ident $(: $FlowWorld:ty)?| $code:expr) => {
        $crate::private::WorldClosureSpawn(|world: &mut $crate::world::World| {
            $crate::private::insert_world_flow(
                world,
                |mut world: $crate::flow::FlowWorld<'static>| async move {
                    #[allow(unused_mut)]
                    let mut  $world $(: $FlowWorld)? = world.reborrow();
                    $code
                },
            )
        })
    };
}

pub fn spawn<F>(world: &mut World, flow_fn: F)
where
    F: WorldFlowSpawn,
{
    flow_fn.spawn(world);
}

pub struct PollWorld<F> {
    f: F,
}

impl<F, R> Future for PollWorld<F>
where
    F: FnMut(&mut World, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            (me.f)(flow_world(), cx)
        }
    }
}

impl<'a> FlowWorld<'a> {
    fn new() -> Self {
        FlowWorld {
            marker: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn reborrow(&mut self) -> FlowWorld<'_> {
        FlowWorld {
            marker: PhantomData,
        }
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    pub fn poll_fn<F, R>(&mut self, f: F) -> PollWorld<F>
    where
        F: FnMut(&mut World, &mut Context) -> Poll<R>,
    {
        PollWorld { f }
    }
}
