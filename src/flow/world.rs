use core::{
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use alloc::boxed::Box;

use crate::{world::WorldLocal, NoSuchEntity};

use super::{BadContext, BadFlowClosure, Entity, Flow, IntoFlow};

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

    /// Polls the world until closure returns [`Poll::Ready`].
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

    /// Polls the world until closure returns [`Poll::Ready`].
    #[inline(always)]
    pub fn poll_fn_mut<F, R>(&mut self, f: F) -> PollWorldMut<F>
    where
        F: FnMut(&WorldLocal, &mut Context) -> Poll<R>,
    {
        PollWorldMut {
            f,
            _world: PhantomData,
        }
    }

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
pub struct BadWorld;

impl BadContext for BadWorld {
    type Context<'a> = &'a mut World;

    fn bad<'a>(&'a self) -> &'a mut World {
        unsafe { World::make_mut() }
    }
}

impl<F, Fut> IntoFlow for BadFlowClosure<F, Fut>
where
    F: FnOnce(BadWorld) -> Fut + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Flow<'a> = FutureFlow<Fut>;

    fn into_flow(self, _world: &mut World) -> Option<FutureFlow<Fut>> {
        Some(FutureFlow {
            fut: (self.f)(BadWorld),
        })
    }
}
