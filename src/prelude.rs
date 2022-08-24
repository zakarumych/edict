//! A prelude module. Reexports types and traits, enough to start using [`edict`]

pub use crate::{
    bundle::{Bundle, DynamicBundle, EntityBuilder},
    component::Component,
    entity::EntityId,
    query::{PhantomQuery, Query, QueryIter},
    relation::Relation,
    world::{EntityError, MissingComponents, NoSuchEntity, QueryOneError, QueryRef, World},
};
