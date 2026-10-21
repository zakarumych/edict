mod alt;
// mod any_of;
mod copied;
mod read;
mod relation;
mod with;
mod write;

use crate::epoch::EpochId;

pub use self::relation::ModifiedRelationFetch;

pub use self::{
    alt::ModifiedFetchAlt, copied::ModifiedFetchCopied, read::ModifiedFetchRead,
    with::ModifiedFetchWith, write::ModifiedFetchWrite,
};

/// Query over modified components or relation sets.
///
/// Relation queries track the backing relation component for each matching entity,
/// not individual edges. `Relates`, `RelatesExclusive`, `RelatesTo` and origin
/// filters track outgoing relations. `Related` and target filters track incoming
/// relations (outgoing relations for symmetric types). The entire matching set is
/// yielded when its component changes. Mutable relation queries touch only their
/// queried direction. Removing the last relation removes the component, so these
/// queries cannot report that removal.
///
/// Component queries can be used as either [`Modified<&T>`], [`Modified<&mut T>`]
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

    /// Creates a modification filter around a query value.
    ///
    /// Use this for queries with a specific relation target or origin, such as
    /// [`RelatesTo`](crate::relation::RelatesTo).
    pub fn with_query(after_epoch: EpochId, query: T) -> Self {
        Self { after_epoch, query }
    }

    /// Epoch id threshold for this query.
    pub fn after_epoch(&self) -> EpochId {
        self.after_epoch
    }
}
