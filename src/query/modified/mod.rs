mod alt;
mod copied;
mod read;
mod with;
mod write;

use core::marker::PhantomData;

use crate::epoch::EpochId;

pub use self::{
    alt::ModifiedFetchAlt, copied::ModifiedFetchCopied, read::ModifiedFetchRead,
    with::ModifiedFetchWith, write::ModifiedFetchWrite,
};

/// Query over modified component.
///
/// Should be used as either [`Modified<&T>`], [`Modified<&mut T>`]
/// or [`Modified<Alt<T>>`].
///
/// This is tracking query that uses epoch lower bound to filter out entities with unmodified components.
pub struct Modified<T> {
    after_epoch: EpochId,
    marker: PhantomData<fn() -> T>,
}

impl_copy!(Modified<T>);
impl_debug!(Modified<T> { after_epoch });

impl<T> Modified<T> {
    /// Creates new `Modified` query.
    /// Provide `after_epoch` id is used to skip components that are last modified not after this epoch.
    pub fn new(after_epoch: EpochId) -> Self {
        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

pub struct ModifiedCache<T> {
    after_epoch: EpochId,
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ModifiedCache<T> {
    fn default() -> Self {
        ModifiedCache {
            after_epoch: EpochId::start(),
            marker: PhantomData,
        }
    }
}
