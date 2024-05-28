use core::{
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use crate::{world::World, Entity, NoSuchEntity};

use super::{flow_world, Flow, FlowEntity, IntoFlow};

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
    fn deref_mut(&mut self) -> &mut World {
        unsafe { flow_world() }
    }
}

/// Future wrapped to be used as a flow.
#[repr(transparent)]
#[doc(hidden)]
pub struct FutureFlow<F> {
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

impl<F> IntoFlow for F
where
    F: for<'a> WorldFlowFn<'a> + Send + 'static,
{
    type Flow = FutureFlow<<F as WorldFlowFn<'static>>::Fut>;

    unsafe fn into_flow(self) -> Self::Flow {
        FutureFlow {
            fut: self.run(FlowWorld::make()),
        }
    }
}

struct BadFutureFlow<F, Fut> {
    f: F,
    _phantom: PhantomData<Fut>,
}

impl<F, Fut> IntoFlow for BadFutureFlow<F, Fut>
where
    F: FnOnce(FlowWorld<'static>) -> Fut + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Flow = FutureFlow<Fut>;

    unsafe fn into_flow(self) -> Self::Flow {
        FutureFlow {
            fut: (self.f)(FlowWorld::make()),
        }
    }
}

pub unsafe fn bad_world_flow_closure<F, Fut>(f: F) -> impl IntoFlow
where
    F: FnOnce(FlowWorld<'static>) -> Fut + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    BadFutureFlow {
        f,
        _phantom: PhantomData,
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
        unsafe {
            $crate::flow::bad_world_flow_closure(move |mut world: $crate::flow::FlowWorld<'static>| async move {
                #[allow(unused_mut)]
                let mut $world $(: $FlowWorld)? = world.reborrow();
                let res: $ret = { $code };
                res
            })
        }
    };
    (|mut $world:ident $(: $FlowWorld:ty)?| $code:expr) => {
        unsafe {
            $crate::flow::bad_world_flow_closure(move |mut world: $crate::flow::FlowWorld<'static>| async move {
                #[allow(unused_mut)]
                let mut $world $(: $FlowWorld)? = world.reborrow();
                $code
            })
        }
    };
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
    #[doc(hidden)]
    pub unsafe fn make() -> Self {
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

    pub fn entity(&self, entity: impl Entity) -> Result<FlowEntity<'_>, NoSuchEntity> {
        let id = entity.id();
        unsafe {
            if !flow_world().is_alive(id) {
                Err(NoSuchEntity)
            } else {
                Ok(FlowEntity::make(id))
            }
        }
    }
}
