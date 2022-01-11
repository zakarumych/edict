//!
//! Edict ECS
//!
// #![no_std]

extern crate alloc;
extern crate self as edict;

pub mod archetype;
pub mod bundle;
pub mod component;
pub mod entity;
pub mod proof;
pub mod query;
pub mod tracks;
pub mod typeidset;
pub mod world;

mod hash;
mod idx;

pub use self::component::{Component, PinComponent, UnpinComponent};
