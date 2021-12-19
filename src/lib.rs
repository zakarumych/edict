//!
//! Edict ECS
//!

extern crate self as edict;

pub mod component;
pub mod entity;
pub mod proof;
pub mod query;
pub mod world;

pub use self::component::{Component, PinComponent, UnpinComponent};
