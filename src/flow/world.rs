use core::{
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    world::{World, WorldLocal},
    Entity, NoSuchEntity,
};

use super::{Flow, FlowEntity, IntoFlow};

/// World reference that is updated when flow is polled.
#[repr(transparent)]
pub struct FlowWorld {
    pub(super) marker: PhantomData<World>,
}

unsafe impl Send for FlowWorld {}
unsafe impl Sync for FlowWorld {}

impl Deref for FlowWorld {
    type Target = WorldLocal;

    fn deref(&self) -> &Self::Target {
        unsafe { self.world_ref() }
    }
}

impl DerefMut for FlowWorld {
    fn deref_mut(&mut self) -> &mut WorldLocal {
        unsafe { self.world_mut() }
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
    fn run(self, world: &'a mut FlowWorld) -> Self::Fut;
}

// Must be callable with any lifetime of `FlowWorld` borrow.
impl<'a, F, Fut> WorldFlowFn<'a> for F
where
    F: FnOnce(&'a mut FlowWorld) -> Fut,
    Fut: Future<Output = ()> + Send + 'a,
{
    type Fut = Fut;

    fn run(self, world: &'a mut FlowWorld) -> Fut {
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
            fut: self.run(FlowWorld::make_mut()),
        }
    }
}

struct BadFutureFlow<F, Fut> {
    f: F,
    _phantom: PhantomData<Fut>,
}

impl<F, Fut> IntoFlow for BadFutureFlow<F, Fut>
where
    F: FnOnce(&'static mut FlowWorld) -> Fut + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Flow = FutureFlow<Fut>;

    unsafe fn into_flow(self) -> Self::Flow {
        FutureFlow {
            fut: (self.f)(FlowWorld::make_mut()),
        }
    }
}

pub unsafe fn bad_world_flow_closure<F, Fut>(f: F) -> impl IntoFlow
where
    F: FnOnce(&'static mut FlowWorld) -> Fut + 'static,
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
    (|mut $world:ident $(: &mut $FlowWorld:ty)?| -> $ret:ty $code:block) => {
        unsafe {
            $crate::flow::bad_world_flow_closure(move |world: &'static mut $crate::flow::FlowWorld| async move {
                #[allow(unused_mut)]
                let $world $(: &mut $FlowWorld)? = &mut*world;
                let res: $ret = { $code };
                res
            })
        }
    };
    (|mut $world:ident $(: &mut $FlowWorld:ty)?| $code:expr) => {
        unsafe {
            $crate::flow::bad_world_flow_closure(move |world: &'static mut $crate::flow::FlowWorld| async move {
                #[allow(unused_mut)]
                let $world $(: &mut $FlowWorld)? = &mut*world;
                $code
            })
        }
    };
}

pub struct PollWorld<'a, F> {
    f: F,
    _world: PhantomData<&'a World>,
}

impl<'a, F, R> Future for PollWorld<'a, F>
where
    F: FnMut(&World, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            (me.f)(super::flow_world_ref(), cx)
        }
    }
}

pub struct PollWorldMut<'a, F> {
    f: F,
    _world: PhantomData<&'a mut World>,
}

impl<'a, F, R> Future for PollWorldMut<'a, F>
where
    F: FnMut(&mut World, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            (me.f)(super::flow_world_mut(), cx)
        }
    }
}

impl FlowWorld {
    #[inline(always)]
    fn world_ref(&self) -> &WorldLocal {
        unsafe { super::flow_world_ref() }
    }

    #[inline(always)]
    fn world_mut(&mut self) -> &mut WorldLocal {
        unsafe { super::flow_world_mut() }
    }

    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn make_mut() -> &'static mut Self {
        // ZST allocation is no-op.
        Box::leak(Box::new(FlowWorld {
            marker: PhantomData,
        }))
    }

    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn make_ref() -> &'static Self {
        &FlowWorld {
            marker: PhantomData,
        }
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    #[inline(always)]
    pub fn poll_fn<F, R>(&self, f: F) -> PollWorld<F>
    where
        F: FnMut(&World, &mut Context) -> Poll<R>,
    {
        PollWorld {
            f,
            _world: PhantomData,
        }
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    #[inline(always)]
    pub fn poll_fn_mut<F, R>(&mut self, f: F) -> PollWorldMut<F>
    where
        F: FnMut(&World, &mut Context) -> Poll<R>,
    {
        PollWorldMut {
            f,
            _world: PhantomData,
        }
    }

    #[inline]
    pub fn flow_entity(&mut self, entity: impl Entity) -> Result<FlowEntity<'_>, NoSuchEntity> {
        let id = entity.id();
        unsafe {
            if self.world_ref().is_alive(id) {
                Ok(FlowEntity::make(id))
            } else {
                Err(NoSuchEntity)
            }
        }
    }
}
