//! A prelude module. Reexports types and traits, enough to start using [`edict`]

pub use crate::{
    bundle::{Bundle, DynamicBundle, EntityBuilder},
    component::Component,
    entity::EntityId,
    query::{ImmutableQuery, Query, QueryItem, QueryIter},
    world::{EntityError, MissingComponents, NoSuchEntity, QueryOneError, World},
};

#[cfg(feature = "rc")]
pub use crate::{
    entity::{Entity, SharedEntity},
    proof::Skip,
    world::OwnershipError,
};

#[cfg(feature = "relation")]
pub use crate::relation::Relation;
