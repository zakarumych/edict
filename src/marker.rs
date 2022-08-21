#![doc(hidden)]

use core::{
    fmt::{self, Debug},
    marker::PhantomData,
};

/// Parameter type for `Marker` trait.
/// Implements [`core::marker::Send`].
///
/// [`T: Marker<Send>`] bound is equivalent to [`T: Send`]
///
/// ```
/// # use edict::marker::Send;
/// fn test_sync<T: core::marker::Sync>() {}
/// test_sync::<Send>();
/// ```
///
/// ```
/// # use edict::marker::Send;
/// fn test_send<T: core::marker::Send>() {}
/// test_send::<Send>();
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Send;

/// Parameter type for `Marker` trait.
/// Does not implement [`core::marker::Send`].
///
/// [`T: Marker<NoSend>`] bound is equivalent to no bounds at all.
///
/// ```
/// # use edict::marker::NoSend;
/// fn test_sync<T: core::marker::Sync>() {}
/// test_sync::<NoSend>();
/// ```
///
/// ```compile_fail
/// # use edict::marker::NoSend;
/// fn test_send<T: core::marker::Send>() {}
/// test_send::<NoSend>();
/// ```
#[derive(Clone, Copy)]
pub struct NoSend {
    marker: PhantomData<*const u8>,
}

// # Safety
//
// The `NoSend` is ZST and has no runtime effects.
// It could implement [`core::marker::Send`] and [`core::marker::Sync`] automatically,
// but was manually opt-out using `PhantomData<*const u8>`.
//
// This manually opt-in to `Sync`.
unsafe impl Sync for NoSend {}

impl Debug for NoSend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NoSend")
    }
}

/// Marker trait bound generic over marker type.
pub trait Marker<T> {}

impl<T> Marker<Send> for T where T: core::marker::Send {}
impl<T> Marker<NoSend> for T {}

#[test]
fn test_send() {
    fn test_send<T: core::marker::Send>() {}
    test_send::<Send>();
}

#[test]
fn test_sync() {
    fn test_sync<T: core::marker::Sync>() {}
    test_sync::<Send>();
    test_sync::<NoSend>();
}

#[test]
fn test_self() {
    fn is_marker<T: Marker<T>>() {}
    is_marker::<Send>();
    is_marker::<NoSend>();
}
