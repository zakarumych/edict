//! Provides API to define systems compatible with built-in scheduler.

mod func;

use alloc::vec::Vec;
use amity::ring_buffer::RingBuffer;
use core::{any::TypeId, ptr::NonNull};
use smallvec::SmallVec;

use crate::{
    action::{ActionBuffer, ActionBufferSliceExt},
    archetype::Archetype,
    component::ComponentInfo,
    world::World,
    Access,
};

pub use self::func::{
    ActionEncoderState, FnArg, FnArgState, FromWorld, IsFunctionSystem, QueryArg, ResLocal,
    ResMutLocal, ResMutNoSendState, ResMutState, ResNoSyncState, ResState, State, StateState,
};

pub use edict_proc::system;

/// A queue of `ActionEncoder` instances.
/// The nature of queue depends on scheduler implementation.
/// Systems must work with any action queue type - the API uses `dyn ActionBufferQueue`.
pub trait ActionBufferQueue {
    /// Returns action encoder from the queue.
    fn get(&mut self) -> ActionBuffer;

    /// Flushes action encoder back to the queue.
    fn flush(&mut self, buffer: ActionBuffer);
}

impl ActionBufferQueue for Vec<ActionBuffer> {
    fn get(&mut self) -> ActionBuffer {
        self.pop().unwrap_or_default()
    }

    fn flush(&mut self, buffer: ActionBuffer) {
        self.push(buffer);
    }
}

impl ActionBufferQueue for RingBuffer<ActionBuffer> {
    fn get(&mut self) -> ActionBuffer {
        self.pop().unwrap_or_default()
    }

    fn flush(&mut self, buffer: ActionBuffer) {
        self.push(buffer);
    }
}

impl<const N: usize> ActionBufferQueue for SmallVec<[ActionBuffer; N]>
where
    [ActionBuffer; N]: smallvec::Array<Item = ActionBuffer>,
{
    fn get(&mut self) -> ActionBuffer {
        self.pop().unwrap_or_default()
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
    #[must_use]
    fn is_local(&self) -> bool;

    /// Returns access type performed on the entire [`World`].
    /// Most systems will return some [`Access::Read`], and few will return none.
    #[must_use]
    fn world_access(&self) -> Option<Access>;

    /// Checks if any query of this system will visit specified archetype.
    #[must_use]
    fn visit_archetype(&self, archetype: &Archetype) -> bool;

    /// Returns access type to the specified component type this system may perform.
    #[must_use]
    fn component_access(&self, archetype: &Archetype, comp: &ComponentInfo) -> Option<Access>;

    /// Returns access type to the specified resource type this system may perform.
    #[must_use]
    fn resource_type_access(&self, ty: TypeId) -> Option<Access>;

    /// Runs the system with given context instance.
    ///
    /// # Safety
    ///
    /// World pointer must be valid and safe to dereference.
    /// If `is_local()` returns `true` then running it outside local thread is unsound.
    unsafe fn run_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionBufferQueue);

    /// Runs the system with exclusive access to [`World`].
    fn run(&mut self, world: &mut World, queue: &mut dyn ActionBufferQueue) {
        unsafe { self.run_unchecked(NonNull::from(world), queue) };
    }

    /// Runs the system with exclusive access to [`World`] and flushes action buffers immediately.
    fn run_alone(&mut self, world: &mut World) {
        let mut buffers = Vec::new();
        self.run(world, &mut buffers);
        buffers.execute_all(world);
    }
}

/// Trait for types that can be converted into systems.
#[diagnostic::on_unimplemented(
    message = "Type must be convertible into a system",
    note = "If this is a function ensure that all arguments implement `FnArg`"
)]
pub trait IntoSystem<Marker> {
    /// Type of the system a value of this type can be converted into.
    type System: System + Send + 'static;

    /// Converts value into system.
    #[must_use]
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

    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    fn component_access(&self, _archetype: &Archetype, _comp: &ComponentInfo) -> Option<Access> {
        Some(Access::Write)
    }

    fn resource_type_access(&self, _ty: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    unsafe fn run_unchecked(
        &mut self,
        mut world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) {
        // Safety: Declares write access and local execution.
        let world = unsafe { world.as_mut() };
        self.system.run(world);
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

/// A system that runs multiple systems in sequence.
pub struct SystemSequence<T>(pub T);

macro_rules! impl_system {
    () => {};
    ($($a:ident)+) => {
        #[allow(non_snake_case)]
        impl< Marker, $($a),+ > IntoSystem<Marker> for ($($a,)+)
        where
            $($a: IntoSystem<Marker>,)+
        {
            type System = SystemSequence<($($a::System,)+)>;

            fn into_system(self) -> Self::System {
                let ($($a,)+) = self;
                SystemSequence(($($a.into_system(),)+))
            }
        }

        #[allow(non_snake_case)]
        unsafe impl< $($a),+ > System for SystemSequence<($($a,)+)>
        where
            $($a: System,)+
        {
            fn is_local(&self) -> bool {
                let ($($a,)+) = &self.0;
                true $(&& $a.is_local())+
            }

            #[inline]
            fn world_access(&self) -> Option<Access> {
                let ($($a,)+) = &self.0;
                let mut result = None;
                $(
                    result = match (result, $a.world_access()) {
                        (Some(Access::Write), _) | (_, Some(Access::Write)) => Some(Access::Write),
                        (Some(Access::Read), _) | (_, Some(Access::Read)) => Some(Access::Read),
                        (None, None) => None,
                    };
                )+
                result
            }

            #[inline]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)*) = &self.0;
                false $(|| $a.visit_archetype(archetype))+
            }

            #[inline]
            fn component_access(&self, archetype: &Archetype, comp: &ComponentInfo) -> Option<Access> {
                let ($($a,)+) = &self.0;
                let mut result = None;
                $(
                    result = match (result, $a.component_access(archetype, comp)) {
                        (Some(Access::Write), _) | (_, Some(Access::Write)) => Some(Access::Write),
                        (Some(Access::Read), _) | (_, Some(Access::Read)) => Some(Access::Read),
                        (None, None) => None,
                    };
                )+
                result
            }

            /// Returns access type to the specified resource type this system may perform.
            #[inline]
            fn resource_type_access(&self, ty: TypeId) -> Option<Access> {
                let ($($a,)+) = &self.0;
                let mut result = None;
                $(
                    result = match (result, $a.resource_type_access(ty)) {
                        (Some(Access::Write), _) | (_, Some(Access::Write)) => Some(Access::Write),
                        (Some(Access::Read), _) | (_, Some(Access::Read)) => Some(Access::Read),
                        (None, None) => None,
                    };
                )+
                result
            }

            /// Runs the system with given context instance.
            ///
            /// If `is_local()` returns `true` then running it outside local thread is unsound.
            #[inline]
            unsafe fn run_unchecked(&mut self, world: NonNull<World>, queue: &mut dyn ActionBufferQueue) {
                let ($($a,)+) = &mut self.0;
                unsafe {
                    $($a.run_unchecked(world, queue);)+
                }
            }
        }
    };
}

for_tuple!(impl_system);
