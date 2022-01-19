//! A prelude module. Reexports types and traits, enough to start using [`edict`]

pub use crate::{
    bundle::{Bundle, DynamicBundle},
    component::Component,
    entity::{Entity, EntityId, WeakEntity},
    query::{
        Alt, ImmutableQuery, Modifed, NonTrackingQuery, Query, QueryItem, QueryIter,
        QueryTrackedIter,
    },
    world::{EntityError, Tracks, World},
};
