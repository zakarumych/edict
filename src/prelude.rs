//! A prelude module. Reexports types and traits, enough to start using [`edict`]
#[doc(no_inline)]
pub use crate::{
    action::{ActionBuffer, ActionBufferSliceExt, ActionEncoder},
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle, EntityBuilder},
    component::Component,
    entity::EntityId,
    query::{Alt, Entities, Modified, PhantomQuery, Query, QueryIter},
    relation::{ChildOf, Related, Relates, RelatesExclusive, RelatesTo, Relation},
    system::{IntoSystem, Res, ResMut, ResMutNoSend, ResNoSync, State, System},
    world::{EntityError, MissingComponents, NoSuchEntity, QueryOneError, QueryRef, World},
};

#[cfg(feature = "std")]
pub use crate::{
    scheduler::Scheduler,
    task::{task_system, task_world, Task},
};