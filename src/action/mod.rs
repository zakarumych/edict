//! This module contains definitions for action recording.
//! Actions can be recorded into [`ActionEncoder`] and executed later onto the [`World`].
//! Two primary use cases for actions are:
//! * Deferring [`World`] mutations when [`World`] is borrowed immutably, like in most [`Systems`]
//! * Generating actions in custom component drop-glue.
//!
//! [`Systems`]: edict::system::System

use crate::world::World;

mod buffer;
mod channel;
mod encoder;

tiny_fn::tiny_fn! {
    pub(crate) struct ActionFn = FnOnce(world: &mut World) | + Send;
    pub(crate) struct LocalActionFn = FnOnce(world: &mut World);
}

pub use self::{
    buffer::{ActionBuffer, ActionBufferSliceExt, LocalActionBuffer},
    channel::{ActionSender, SpawnBatchSender},
    encoder::{ActionEncoder, LocalActionEncoder, LocalSpawnBatch, SpawnBatch},
};

pub(crate) use self::channel::ActionChannel;
