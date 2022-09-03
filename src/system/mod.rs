//! Provides API to define systems compatible with built-in scheduler.

mod func;

use core::{any::TypeId, ptr::NonNull};

use crate::{action::ActionEncoder, archetype::Archetype, query::Access, world::World};

pub use self::func::{
    FnArg, FnArgCache, FnArgGet, FromWorld, IsFunctionSystem, QueryArg, QueryArgCache, QueryArgGet,
    QueryRefCache, Res, ResCache, ResMut, ResMutCache, ResMutNoSend, ResMutNoSendCache, ResNoSync,
    ResNoSyncCache, State, StateCache,
};

/// A queue of `ActionEncoder` instances.
/// The nature of queue depends on scheduler implementation.
/// Systems must work with any action queue type - the API uses `dyn ActionQueue`.
pub trait ActionQueue {
    /// Returns action encoder from the queue.
    fn get_action_encoder(&self) -> ActionEncoder;

    /// Flushes action encoder back to the queue.
    fn flush_action_encoder(&mut self, encoder: ActionEncoder);
}

impl ActionQueue for Vec<ActionEncoder> {
    fn get_action_encoder(&self) -> ActionEncoder {
        ActionEncoder::new()
    }

    fn flush_action_encoder(&mut self, encoder: ActionEncoder) {
        self.push(encoder);
    }
}

/// System that can run using reference to [`World`].
///
/// [`System::is_local`] method returns `true` for local systems.
/// Such system may be run only on thread where [`World`] lives.
/// [`System::run`] call is unsafe, as running it outside local thread is unsound for local systems.
///
/// # Safety
///
/// If [`System::is_local()`] returns false [`System::run_unchecked`] must be safe to call from any thread.
/// Otherwise [`System::run_unchecked`] must be safe to call from local thread.
pub unsafe trait System {
    /// Returns `true` for local systems that can be run only on thread where [`World`] lives.
    fn is_local(&self) -> bool;

    /// Returns access type performed on the entire [`World`].
    /// Most arguments will return some [`Access::Read`], and few will return none.
    fn world_access(&self) -> Option<Access>;

    /// Checks if all queries from this system will skip specified archetype.
    fn skips_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns access type to the specified component type this system may perform.
    fn access_component(&self, id: TypeId) -> Option<Access>;

    /// Returns access type to the specified resource type this system may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access>;

    /// Runs the system with given context instance.
    ///
    /// If `is_local()` returns `true` then running it outside local thread is unsound.
    unsafe fn run_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionQueue);
}

/// Trait for types that can be converted into systems.
pub trait IntoSystem<Marker> {
    /// Type of the system a value of this type can be converted into.
    type System: System + Send + 'static;

    /// Converts value into system.
    fn into_system(self) -> Self::System;
}

/// Identity marker for [`IntoSystem`] trait.
pub struct IsSystem;

impl<T> IntoSystem<IsSystem> for T
where
    T: System + Send + 'static,
{
    type System = T;

    fn into_system(self) -> T {
        self
    }
}
