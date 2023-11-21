//! A prelude module. Reexports types and traits, enough to start using [`edict`]
#[doc(no_inline)]
pub use crate::{
    action::{ActionBuffer, ActionBufferSliceExt, ActionEncoder},
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle, EntityBuilder},
    component::Component,
    entity::{Entity, EntityBound, EntityId, EntityLoc, EntityRef},
    query::{Alt, Entities, Modified, Query, With, Without},
    relation::{ChildOf, Related, Relates, RelatesExclusive, RelatesTo, Relation},
    system::{IntoSystem, Res, ResMut, ResMutNoSend, ResNoSync, State, System},
    view::{View, ViewCell, ViewIter, ViewOne},
    world::{World, WorldBuilder},
    EntityError, NoSuchEntity,
};

#[cfg(feature = "std")]
pub use crate::scheduler::Scheduler;
