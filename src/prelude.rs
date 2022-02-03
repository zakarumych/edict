//! A prelude module. Reexports types and traits, enough to start using [`edict`]

pub use crate::{
    bundle::{Bundle, DynamicBundle, EntityBuilder},
    component::Component,
    entity::EntityId,
    query::{
        Alt, ImmutableQuery, Modifed, NonTrackingQuery, Query, QueryItem, QueryIter,
        QueryTrackedIter,
    },
    world::{EntityError, MissingComponents, NoSuchEntity, Tracks, World},
};

#[cfg(feature = "rc")]
pub use crate::{
    entity::{Entity, SharedEntity},
    proof::Skip,
};
