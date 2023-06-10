use crate::{
    archetype::Archetype,
    entity::{AliveEntity, EntityId, EntitySet, Location},
    epoch::EpochCounter,
    query::{IntoQuery, Query},
    world::World,
};

use super::RuntimeBorrowState;

/// A view over [`World`] that may be used to access specific components.
#[must_use]
pub struct ViewOneState<'a, Q: Query, F: Query> {
    query: Q,
    filter: F,
    archetype: &'a Archetype,
    id: EntityId,
    loc: Location,
    entity_set: &'a EntitySet,
    borrow: RuntimeBorrowState,
    epochs: &'a EpochCounter,
}

pub type ViewOne<'a, Q, F = ()> =
    ViewOneState<'a, <Q as IntoQuery>::Query, <F as IntoQuery>::Query>;

impl<'a, Q, F> ViewOne<'a, Q, F> {
    pub fn new(world: &World, entity: impl AliveEntity) -> Self {
        entity.locate(&)
    }
}
