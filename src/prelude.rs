//! A prelude module. Reexports types and traits, enough to start using [`edict`]

pub use crate::{
    action::{ActionEncoder, ActionEncoderSliceExt},
    bundle::{Bundle, DynamicBundle, EntityBuilder},
    component::Component,
    entity::EntityId,
    query::{Alt, Modified, PhantomQuery, Query, QueryIter},
    relation::{Related, Relates, RelatesExclusive, RelatesTo},
    scheduler::Scheduler,
    system::{IntoSystem, Res, ResMut, ResMutNoSend, ResNoSync, State, System},
    world::{EntityError, MissingComponents, NoSuchEntity, QueryOneError, QueryRef, World},
};
