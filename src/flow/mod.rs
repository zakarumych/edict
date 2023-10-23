//! Flow module provides API to create high-level async workflows on top of the Edict ECS.
//!
//! Typical example would be a flow that caues an entity to move towards a target.
//! It is resolved when the entity reaches the target or target is destroyed.
//!
//! Spawning a flow also returns a handle that can be used to await or cancel the flow.
//! Spawned flow wraps flow result in a `Result` type where `Err` signals that the flow was cancelled.
//! Flows are canceled when handle is dropped or entity is despawned.
//!
//! # Example
//!
//! ```
//! unit.move_to(target).await
//! ```
//!

use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    entity::{EntityId, EntityRef},
    world::World,
};

/// Flows spawned on an entity.
struct EntityFlows {}

/// Spawns a flow on the specified entity.
///
/// `f` is a function that returns a flow.
/// It may accept the entity id and
/// references to the entity's components.
///
/// If entity has all components that are
/// required by the function, the function is called.
/// The returned future represents the flow.
///
/// The flow is added to the entity's.
/// When flow is resumed and the entity doesn't have
/// components that were provided to the function,
/// the flow is cancelled.
pub fn spawn<F, T>(entity: EntityRef, f: F)
where
    F: IntoEntityFlow<T>,
{
}

pub trait IntoEntityFlow<T> {}

pub trait Flow {
    fn poll(self: Pin<&mut Self>, world: &World, cx: &mut Context) -> Poll<()>;
}

impl<F, Fut> IntoEntityFlow<()> for F
where
    F: FnOnce() -> Fut,
    Fut: Future,
{
}

impl<F, Fut> IntoEntityFlow<(EntityId,)> for F
where
    F: FnOnce(EntityId) -> Fut,
    Fut: Future,
{
}

impl<F, A, Fut> IntoEntityFlow<(&A,)> for F
where
    F: FnOnce(&A) -> Fut,
    Fut: Future,
{
}
