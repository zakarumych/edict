//! Provides world serialization integration with serialization crates.
//!
//! Supports
//! - `serde`
//! - `nanoserde`
//! - `alkahest`
//!
//! Each can be enabled with a feature named as serialization crate.

use core::{
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use hashbrown::HashMap;

use crate::entity::EntityId;

#[cfg(feature = "alkahest")]
pub mod alkahest;

#[cfg(feature = "nanoserde")]
pub mod nanoserde;

#[cfg(feature = "serde")]
pub mod serde;

/// Dump entity identifier component.
/// The dump will only contain entities with this component.
/// The id itself will be used to map [`EntityId`] to id in the dump.
#[repr(transparent)]
pub struct DumpId<X>(u64, PhantomData<fn() -> X>);

impl<X> Clone for DumpId<X> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<X> Copy for DumpId<X> {}

impl<X> PartialEq for DumpId<X> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<X> Eq for DumpId<X> {}

impl<X> Hash for DumpId<X> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Maps `DumpId` to [`EntityId`].
///
/// May be stored as a resource in the [`World`].
pub struct DumpIdMap<X>(HashMap<u64, EntityId>, PhantomData<fn() -> X>);
