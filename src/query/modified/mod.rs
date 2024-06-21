mod alt;
// mod any_of;
mod copied;
mod read;
mod with;
mod write;

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
#[derive(Clone, Copy, Debug)]
pub struct Modified<T> {
    after_epoch: EpochId,
    query: T,
}

impl<T> Modified<T> {
    /// Creates new `Modified` query.
    /// Uses provided `after_epoch` id to skip components that are last modified not after this epoch.
    pub fn new(after_epoch: EpochId) -> Self
    where
        T: Default,
    {
        Modified {
            after_epoch,
            query: T::default(),
        }
    }

    /// Epoch id threshold for this query.
    pub fn after_epoch(&self) -> EpochId {
        self.after_epoch
    }
}
