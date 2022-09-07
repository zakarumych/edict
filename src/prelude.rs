//! A prelude module. Reexports types and traits, enough to start using [`edict`]
#[doc(no_inline)]
pub use crate::{
    action::{ActionEncoder, ActionEncoderSliceExt},
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle, EntityBuilder},
    component::Component,
    entity::EntityId,
    query::{Alt, Entities, Modified, PhantomQuery, Query, QueryIter},
    relation::{ChildOf, Related, Relates, RelatesExclusive, RelatesTo, Relation},
    scheduler::Scheduler,
    system::{IntoSystem, Res, ResMut, ResMutNoSend, ResNoSync, State, System},
    world::{EntityError, MissingComponents, NoSuchEntity, QueryOneError, QueryRef, World},
};
