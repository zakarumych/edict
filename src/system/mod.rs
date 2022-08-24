//! Provides API to define systems compatible with built-in scheduler.

mod func;

use core::any::TypeId;

use crate::{archetype::Archetype, query::Access, world::World};

pub use self::func::{FnArgExtract, FnSystemArg, FromWorld, State, StateCache};

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

    /// Checks if all queries from this system will skip specified archetype.
    fn skips_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns access type to the specified component type this system may perform.
    fn access_component(&self, id: TypeId) -> Option<Access>;

    /// Returns access type to the specified resource type this system may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access>;

    /// Runs the system with given context instance.
    ///
    /// If `is_local()` returns `true` then running it outside local thread is unsound.
    unsafe fn run_unchecked(&mut self, world: &World);
}

/// Trait for types that can be converted into systems.
pub trait IntoSystem<Marker> {
    /// Type of the system a value of this type can be converted into.
    type System: System + Send + Sync + 'static;

    /// Converts value into system.
    fn into_system(self) -> Self::System;
}
