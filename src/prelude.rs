//! A prelude module. Reexports types and traits, enough to start using [`edict`]
#[doc(no_inline)]
pub use crate::{
    action::{
        ActionBuffer, ActionEncoder, ActionSender, LocalActionBuffer, LocalActionEncoder,
        LocalSpawnBatch, SpawnBatch, SpawnBatchSender,
    },
    bundle::EntityBuilder,
    component::Component,
    entity::{EntityId, EntityRef},
    query::{Alt, Entities, Modified, Query, With, Without},
    relation::{ChildOf, Related, Relates, RelatesExclusive, RelatesTo, Relation},
    resources::{Res, ResMut},
    system::{ResLocal, ResMutLocal, State, System},
    view::{View, ViewCell, ViewCellIter, ViewIter, ViewMut, ViewOne, ViewRef},
    world::{World, WorldBuilder},
    EntityError, NoSuchEntity,
};
