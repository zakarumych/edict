//! Provides API to define systems compatible with built-in scheduler.

mod func;

use core::{any::TypeId, ptr::NonNull};

use crate::{action::ActionBuffer, archetype::Archetype, query::Access, world::World};

pub use self::func::{
    ActionEncoderCache, FnArg, FnArgCache, FnArgGet, FromWorld, IsFunctionSystem, QueryArg,
    QueryArgCache, QueryArgGet, QueryRefCache, Res, ResCache, ResMut, ResMutCache, ResMutNoSend,
    ResMutNoSendCache, ResNoSync, ResNoSyncCache, State, StateCache,
};

/// A queue of `ActionEncoder` instances.
/// The nature of queue depends on scheduler implementation.
/// Systems must work with any action queue type - the API uses `dyn ActionQueue`.
pub trait ActionQueue {
    /// Returns action encoder from the queue.
    fn get<'a>(&self) -> ActionBuffer;

    /// Flushes action encoder back to the queue.
    fn flush(&mut self, buffer: ActionBuffer);
}

impl ActionQueue for Vec<ActionBuffer> {
    fn get(&self) -> ActionBuffer {
        ActionBuffer::new()
    }

    fn flush(&mut self, buffer: ActionBuffer) {
        self.push(buffer);
    }
}

/// System that can run using reference to [`World`].
///
/// [`System::is_local`] method returns `true` for local systems.
/// Such system may be run only on thread where [`World`] lives.
///
/// [`System::run_unchecked`] call is unsafe:
/// * running local system outside local thread may cause undefined behavior.
/// * running system for which [`System::world_access`] returns [`Some(Access::Write)`]
///   in parallel with system for which [`System::world_access`] returns [`Some(_)`] may cause undefined behavior.
///
/// # Safety
///
/// If [`System::is_local()`] returns false [`System::run_unchecked`] must be safe to call from any thread.
/// Otherwise [`System::run_unchecked`] must be safe to call from local thread.
/// [`System::run_unchecked`] must be safe to call in parallel with any system if [`System::world_access`] returns [`None`].
/// [`System::run_unchecked`] must be safe to call in parallel with other systems if for all of them [`System::world_access`] returns [`Some(Access::Read)`].
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
pub enum IsSystem {}

impl<T> IntoSystem<IsSystem> for T
where
    T: System + Send + 'static,
{
    type System = T;

    fn into_system(self) -> T {
        self
    }
}

/// A thread-safe system that can run in parallel with others.
///
/// In contrast with [`System`] incorrect access declaration and archetype skipping
/// can't result in undefined behavior. Instead runtime checks will cause a panic.
pub trait ParallelSystem {
    /// Checks if all queries from this system will skip specified archetype.
    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        let _ = archetype;
        false
    }

    /// Returns access type to the specified component type this system may perform.
    fn access_component(&self, id: TypeId) -> Option<Access> {
        let _ = id;
        None
    }

    /// Returns access type to the specified resource type this system may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        let _ = id;
        None
    }

    /// Runs the system with given context instance.
    ///
    /// If `is_local()` returns `true` then running it outside local thread is unsound.
    fn run(&mut self, world: &World, queue: &mut dyn ActionQueue);
}

/// Marker for [`IntoSystem`] to turn [`ParallelSystem`] into [`System`].
pub enum IsParallelSystem {}

/// Wraps [`ParallelSystem`] and implements [`System`] trait.
pub struct IntoParallelSystem<S> {
    system: S,
}

unsafe impl<S> System for IntoParallelSystem<S>
where
    S: ParallelSystem,
{
    fn is_local(&self) -> bool {
        false
    }

    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        self.system.skips_archetype(archetype)
    }

    fn access_component(&self, id: TypeId) -> Option<Access> {
        self.system.access_component(id)
    }

    fn access_resource(&self, id: TypeId) -> Option<Access> {
        self.system.access_resource(id)
    }

    unsafe fn run_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionQueue) {
        self.system.run(world.as_ref(), queue);
    }
}

impl<S> IntoSystem<IsParallelSystem> for S
where
    S: ParallelSystem + Send + 'static,
{
    type System = IntoParallelSystem<S>;

    fn into_system(self) -> IntoParallelSystem<S> {
        IntoParallelSystem { system: self }
    }
}

/// A thread-local system that cannot run in parallel with others.
/// Local system borrows whole [`World`] mutably.
pub trait LocalSystem {
    /// Runs the system with given context instance.
    ///
    /// If `is_local()` returns `true` then running it outside local thread is unsound.
    fn run(&mut self, world: &mut World);
}

/// Marker for [`IntoSystem`] to turn [`LocalSystem`] into [`System`].
pub enum IsLocalSystem {}

/// Wraps [`LocalSystem`] and implements [`System`] trait.
pub struct IntoLocalSystem<S> {
    system: S,
}

unsafe impl<S> System for IntoLocalSystem<S>
where
    S: LocalSystem,
{
    fn is_local(&self) -> bool {
        true
    }

    fn world_access(&self) -> Option<Access> {
        Some(Access::Write)
    }

    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    fn access_component(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    unsafe fn run_unchecked(&mut self, mut world: NonNull<World>, _queue: &mut dyn ActionQueue) {
        self.system.run(world.as_mut());
    }
}

impl<S> IntoSystem<IsLocalSystem> for S
where
    S: LocalSystem + Send + 'static,
{
    type System = IntoLocalSystem<S>;

    fn into_system(self) -> IntoLocalSystem<S> {
        IntoLocalSystem { system: self }
    }
}
