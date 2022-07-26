//! [`UnsafeAny`] helps to perform type casts with checks or without depending on `debug_assertions` cfg flag.
//! This is mostly similar to use `dyn Any` and `Any::downcast_*_unchecked`.
//!
//! Except [`UnsafeAny`] is [`Sized`] and thus `&UnsafeAny` is thin pointer when `debug_assertions` cfg flag is disabled.

use core::any::Any;

use alloc::sync::Arc;

/// Unsafe version of `dyn` [`Any`] + [`Send`] + [`Sync`].
/// That fallbacks to `dyn Any` when `debug_assertions` are enabled.
/// This allows catching invalid casts in debug mode.
#[repr(transparent)]
pub struct UnsafeAny {
    #[cfg(debug_assertions)]
    inner: dyn Any + Send + Sync,
    #[cfg(not(debug_assertions))]
    inner: u8,
}

impl UnsafeAny {
    pub fn from_arc<T>(ptr: Arc<T>) -> Arc<Self>
    where
        T: Send + Sync + 'static,
    {
        let ptr = Arc::into_raw(ptr);

        #[cfg(debug_assertions)]
        let inner = ptr as *const (dyn Any + Send + Sync);

        #[cfg(not(debug_assertions))]
        let inner = ptr as *const u8;

        unsafe { Arc::from_raw(inner as *const Self) }
    }

    pub unsafe fn downcast_ref_unchecked<T>(&self) -> &T
    where
        T: 'static,
    {
        debug_assert!(self.inner.is::<T>());
        &*(self as *const Self as *const T)
    }
}
