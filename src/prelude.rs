//! A prelude module. Reexports types and traits, enough to start using [`edict`]
#[doc(no_inline)]
pub use crate::{
    action::{ActionBuffer, ActionBufferSliceExt, ActionEncoder},
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle, EntityBuilder},
    component::Component,
    entity::{Entity, EntityBound, EntityId, EntityLoc, EntityRef},
    query::{Alt, Entities, Modified, Query},
    relation::{ChildOf, Related, Relates, RelatesExclusive, RelatesTo, Relation},
    scheduler::Scheduler,
    system::{IntoSystem, Res, ResMut, ResMutNoSend, ResNoSync, State, System},
    task::{task_system, task_world, Task},
    view::{View, ViewCell, ViewIter, ViewOne},
    world::{World, WorldBuilder},
};
