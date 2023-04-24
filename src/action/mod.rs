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
    struct ActionFn = FnOnce(world: &mut World, buffer: &mut ActionBuffer) | + Send;
}

pub use self::{
    buffer::{ActionBuffer, ActionBufferSliceExt},
    channel::{ActionSender, SpawnBatchChannel},
    encoder::{ActionEncoder, SpawnBatch},
};

pub(crate) use self::channel::ActionChannel;
