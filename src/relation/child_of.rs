use super::{ExclusiveRelation, Relation};

/// Child -> Parent relation.
/// Children can have only one parent. So this relation is exclusive.
/// Children should be despawned when parent is despawned. So this relation is owned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChildOf;

impl Relation for ChildOf {
    const EXCLUSIVE: bool = true;
    const OWNED: bool = true;
    const SYMMETRIC: bool = false;
}

impl ExclusiveRelation for ChildOf {}
