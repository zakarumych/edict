use core::{
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use alloc::boxed::Box;

use crate::{world::WorldLocal, NoSuchEntity};

use super::{Entity, Flow, FlowClosure, FlowContext, IntoFlow};

/// World reference that is updated when flow is polled.
#[repr(transparent)]
pub struct World {
    pub(super) marker: PhantomData<WorldLocal>,
}

unsafe impl Send for World {}
unsafe impl Sync for World {}

impl Deref for World {
    type Target = WorldLocal;

    fn deref(&self) -> &WorldLocal {
        unsafe { self.world_ref() }
    }
}

impl DerefMut for World {
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
    F: Future<Output = ()> + Send,
{
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = unsafe { self.get_unchecked_mut() };
        let fut = unsafe { Pin::new_unchecked(&mut this.fut) };
        let poll = fut.poll(cx);
        poll
    }
}

/// Trait implemented by functions that can be used to spawn flows.
/// Argument represents the world that can be used to fetch entities, components and resources.
pub trait WorldFlowFn<'a> {
    /// Future type returned from the function.
    type Fut: Future<Output = ()> + Send + 'a;

    /// Runs the function with world reference.
    fn run(self, world: &'a mut World) -> Self::Fut;
}

// Must be callable with any lifetime of `World` borrow.
impl<'a, F, Fut> WorldFlowFn<'a> for F
where
    F: FnOnce(&'a mut World) -> Fut,
    Fut: Future<Output = ()> + Send + 'a,
{
    type Fut = Fut;

    fn run(self, world: &'a mut World) -> Fut {
        self(world)
    }
}

impl<F> IntoFlow for F
where
    F: for<'a> WorldFlowFn<'a> + Send + 'static,
{
    type Flow<'a> = FutureFlow<<F as WorldFlowFn<'a>>::Fut>;

    fn into_flow<'a>(self, world: &'a mut World) -> Option<Self::Flow<'a>> {
        Some(FutureFlow {
            fut: self.run(world),
        })
    }
}

/// Flow future that provides world to the bound closure on each poll.
pub struct PollWorld<'a, F> {
    f: F,
    _world: PhantomData<fn() -> &'a World>,
}

impl<'a, F, R> Future for PollWorld<'a, F>
where
    F: FnMut(&WorldLocal, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            (me.f)(super::flow_world_ref(), cx)
        }
    }
}

/// Flow future that provides world to the bound closure on each poll.
pub struct PollWorldMut<'a, F> {
    f: F,
    _world: PhantomData<fn() -> &'a mut World>,
}

impl<'a, F, R> Future for PollWorldMut<'a, F>
where
    F: FnMut(&mut WorldLocal, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            (me.f)(super::flow_world_mut(), cx)
        }
    }
}

impl World {
    #[inline(always)]
    fn world_ref(&self) -> &WorldLocal {
        unsafe { super::flow_world_ref() }
    }

    #[inline(always)]
    fn world_mut(&mut self) -> &mut WorldLocal {
        unsafe { super::flow_world_mut() }
    }

    #[inline(always)]
    pub(super) unsafe fn make_mut() -> &'static mut Self {
        // ZST allocation is no-op.
        Box::leak(Box::new(World {
            marker: PhantomData,
        }))
    }

    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn make_ref() -> &'static Self {
        &World {
            marker: PhantomData,
        }
    }

    /// Polls provided closure until it returns [`Poll::Ready`].
    /// Provides reference to the world to the closure on each call.
    #[inline(always)]
    pub fn poll_fn<F, R>(&self, f: F) -> PollWorld<F>
    where
        F: FnMut(&WorldLocal, &mut Context) -> Poll<R>,
    {
        PollWorld {
            f,
            _world: PhantomData,
        }
    }

    /// Polls provided closure until it returns [`Poll::Ready`].
    /// Provides mutable reference to the world to the closure on each call.
    #[inline(always)]
    pub fn poll_fn_mut<F, R>(&mut self, f: F) -> PollWorldMut<F>
    where
        F: FnMut(&mut WorldLocal, &mut Context) -> Poll<R>,
    {
        PollWorldMut {
            f,
            _world: PhantomData,
        }
    }

    /// Returns entity reference.
    /// Returns [`NoSuchEntity`] error if entity is not alive.
    #[inline]
    pub fn entity(
        &mut self,
        entity: impl crate::entity::Entity,
    ) -> Result<Entity<'_>, NoSuchEntity> {
        let id = entity.id();
        unsafe {
            if self.world_ref().is_alive(id) {
                Ok(Entity::make(id))
            } else {
                Err(NoSuchEntity)
            }
        }
    }
}

#[doc(hidden)]
pub struct FlowWorld;

impl<'a> FlowContext<'a> for &'a mut World {
    type Token = FlowWorld;

    fn cx(_token: &'a FlowWorld) -> Self {
        unsafe { World::make_mut() }
    }
}

impl<F, Fut> IntoFlow for FlowClosure<F, Fut>
where
    F: FnOnce(FlowWorld) -> Fut + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Flow<'a> = FutureFlow<Fut>;

    fn into_flow(self, _world: &mut World) -> Option<FutureFlow<Fut>> {
        Some(FutureFlow {
            fut: (self.f)(FlowWorld),
        })
    }
}
