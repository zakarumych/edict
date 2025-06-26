use core::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use smallvec::SmallVec;

use crate::component::Component;

use super::FlowEntity;

/// Future that yields control to the executor once.
pub struct YieldNow {
    yielded: bool,
}

impl Default for YieldNow {
    fn default() -> Self {
        Self::new()
    }
}

impl YieldNow {
    /// Create a new instance of [`YieldNow`] future.
    pub const fn new() -> Self {
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

/// Yield control to the executor once.
/// Useful for implementing waiting in loops
/// when there's no where to add a [`Waker`].
#[macro_export]
macro_rules! yield_now {
    () => {
        $crate::flow::YieldNow::new().await
    };
}

/// Component to wake all wakers bound to this entity when it's dropped.
pub struct WakeOnDrop {
    wakers: SmallVec<[Waker; 4]>,
}

impl Default for WakeOnDrop {
    fn default() -> Self {
        Self::new()
    }
}

impl WakeOnDrop {
    /// Create a new instance of [`WakeOnDrop`] component.
    pub fn new() -> Self {
        WakeOnDrop {
            wakers: SmallVec::new(),
        }
    }

    /// Add a waker to the list of wakers to wake when this component is dropped.
    ///
    /// If waker bound to the same task is already in the list, it will not be added again.
    pub fn add_waker(&mut self, waker: &Waker) {
        for elem in &mut self.wakers {
            if elem.will_wake(waker) {
                return;
            }
        }
        self.wakers.push(waker.clone());
    }

    /// Remove a waker from the list of wakers to wake when this component is dropped.
    pub fn remove_waker(&mut self, waker: &Waker) {
        if let Some(idx) = self.wakers.iter().position(|elem| elem.will_wake(waker)) {
            self.wakers.swap_remove(idx);
        }
    }
}

impl Drop for WakeOnDrop {
    fn drop(&mut self) {
        // Wake all flows bound to this entity to
        // allow them to terminate.
        for waker in self.wakers.drain(..) {
            waker.wake();
        }
    }
}

impl Component for WakeOnDrop {
    #[inline]
    fn name() -> &'static str {
        "WakeOnDrop"
    }
}

impl FlowEntity {
    /// Waits until the entity is despawned.
    pub async fn wait_despawned(self) {
        let r = self
            .try_poll(|_e, _cx| {
                Poll::<Infallible>::Pending // `try_poll` will resolve to None when entity is despawned.
            })
            .await;

        if let Ok(never) = r {
            match never {}
        }
    }

    /// Waits until the entity gets a component.
    /// Never resolves if the entity is despawned.
    pub async fn wait_has_component<T>(self)
    where
        T: 'static,
    {
        self.poll_view::<&T, _, _>(|_, _cx| Poll::Ready(())).await;
    }
}
