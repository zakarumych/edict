//! Edict is a fast and powerful ECS crate that expands traditional ECS feature set.
//! Written in Rust by your fellow 🦀
//!
//! # Features
//!
//! ### General purpose
//!
//! Archetype based ECS with fast iteration and ergonomics in mind.
//!
//! ### Simple IDs
//!
//! [`EntityId`] as a unique identifier of an entity.
//! Edict uses unique IDs without generation and recycling.
//! This greatly simplifying serialization of the [`World`]'s state as it doesn't require any processing of entity IDs.
//!
//! IDs are allocated in sequence from [`IdRange`]s that are allocated by [`IdRangeAllocator`].
//! By default [`IdRange`] that spans from 1 to `u64::MAX` is used.
//! Custom [`IdRangeAllocator`] can be provided to [`WorldBuilder`] to use custom ID ranges.
//!
//! For example in client-server architecture, server and client may use non-overlapping ID ranges.
//! Thus allowing state serialized on server to be transferred to client without ID mapping,
//! which can be cumbersome when components reference entities.
//!
//! In multi-server or p2p architecture [`IdRangeAllocator`] would need to communicate to allocate disjoint ID ranges for each server.
//!
//! ### Ergonomic entity types
//!
//! [`Entity`] and [`AliveEntity`] traits implemented for entity types.
//! Entity types provide convenient guaranties for entity existence and its location.
//!
//! [`EntityId`] implements only [`Entity`] as it doesn't provide any guaranties.
//!
//! [`EntityBound`] is guaranteed to be alive, allowing using it in methods that doesn't handle entity absence.
//! Using it with wrong [`World`] may cause panic.
//!
//! [`EntityLoc`] not only guarantees entity existence but also provides location of the entity in the archetypes,
//! allowing to skip lookup step when accessing its components.
//! Using it with wrong [`World`] may cause panic.
//!
//! [`EntityRef`] is special.
//! It doesn't implement [`Entity`] or [`AliveEntity`] traits since it should be used in world methods.
//! Instead it provides direct access to entity's data and allows mutations such as inserting/removing components.
//!
//! ### Flexible queries
//!
//! Powerful [`Query`] mechanism that can filter entities by components, relations and other criteria and fetch entity data.
//! Queries can be mutable or immutable, sendable or non-sendable, stateful or stateless.
//! Using query on [`World`] creates [`View`]s that can be used to iterate over entities that match the query yielding query items.
//!
//! ### Resources
//!
//! Built-in type-map for singleton values called "resources".
//! Resources can be inserted into/fetched from [`World`].
//! Resources live separately from entities and their components.
//!
//! ### Non-thread-safe types
//!
//! Support for [`!Send`] and [`!Sync`] components and resources with some limitations.
//!
//! [`World`] itself is not sendable but shareable between threads via [`WorldShare`] wrapper.
//! Thread owning [`World`] is referred as "main" thread.
//!
//! Components and resources that are [`!Send`] can be fetched mutably only from "main" thread.
//! Components and resources that are [`!Sync`] can be fetched immutably only from "main" thread.
//! Since reference to [`World`] may exist outside "main" thread, [`WorldLocal`] reference should be used,
//! it can be created using mutable reference to [`World`].
//!
//! ### Automatic change tracking.
//!
//! Each component instance is equipped with epoch counter that tracks last potential mutation of the component.
//! Queries may read and update components epoch to track changes.
//! Queries to filter recently changed components are provided with [`Modified`] type.
//! Last epoch can be obtained with [`World::epoch`].
//!
//! ### Entity relations
//!
//! A relation can be added to pair of entities, binding them together.
//! Queries may fetch relations and filter entities by their relations to other entities.
//! When either of the two entities is despawned, relation is dropped.
//! [`Relation`] type may further configure behavior of the bounded entities.
//!
//! ### Runtime and compile time checks
//!
//! Runtime checks for query validity and mutable aliasing avoidance.
//! [`ViewCell`] with runtime checks allows multiple views with aliased access coexist,
//! deferring checks to runtime that prevents invalid aliasing to occur.
//!
//! When this is not required, [`View`]s with compile time checks should be used instead.
//!
//! ### Deferred actions
//!
//! Use [`ActionEncoder`] for recording actions and run them later with mutable access to [`World`].
//! Or [`LocalActionEncoder`] instead when action is not [`Send`].
//! Or convenient [`WorldLocal::defer*`] methods to defer actions to internal [`LocalActionEncoder`].
//!
//! ### Customizable
//!
//! [`WorldBuilder`] provides opportunity to override some behavior.
//! See below for details.
//!
//! ### Components with trait and without
//!
//! Optional [`Component`] trait that allows implicit component type registration when component is inserted first time.
//! Implicit registration uses behavior defined by [`Component`] implementation as-is.
//! When needed, explicit registration can be done using [`WorldBuilder`] to override component behavior.
//!
//! Non [`Component`] types require explicit registration and
//! few methods with `_external` suffix is used with them instead of normal ones.
//! Only default registration is possible when [`World`] is already built.
//! When needed, explicit registration can be done using [`WorldBuilder`] to override component behavior.
//!
//! ### Hooks
//!
//! Component replace/drop hooks are called automatically when component is replaced or dropped.
//!
//! When component is registered it can be equipped with hooks to be called when component value is replaced or dropped.
//! Implicit registration of [`Component`] types will register hooks defined on the trait impl.
//!
//! Drop hook is called when component is dropped via `World::drop` or entity is despawned and is not
//! called when component is removed from entity.
//!
//! Replace hook is called when component is replaced e.g. component is inserted into entity
//! and entity already has component of the same type.
//! Replace hook returns boolean value that indicates if drop hook should be called for replaced component.
//!
//! Hooks can record actions into provided [`LocalActionEncoder`] that will be executed
//! before [`World`] method that caused the hook to be called returns.
//!
//! When component implements [`Component`] trait, hooks defined on the trait impl are registered automatically to call
//! [`Component::on_drop`] and [`Component::on_replace`] methods.
//! They may be overridden with custom hooks using [`WorldBuilder`].
//! For non [`Component`] types hooks can be registered only via [`WorldBuilder`].
//! Default registration with [`World`] will not register any hooks.
//!
//! ### Borrows
//!
//! Component type may define borrowing operations to borrow another type from it.
//! Borrowed type may be not sized, allowing slices and dyn traits to be borrowed.
//! A macro to help define borrowing operations is provided.
//! Queries that tries to borrow type from suitable components are provided:
//! * [`BorrowAll`] borrows from all components that implement borrowing requested type.
//!   Yields a `Vec` with borrowed values since multiple components of the entity may provide it.
//!   Skips entities if none of the components provide the requested type.
//! * [`BorrowAny`] borrows from first suitable component that implements borrowing requested type.
//!   Yields a single value.
//!   Skips entities if none of the components provide the requested type.
//! * [`BorrowOne`] is configured with [`TypeId`] of component from which it should borrow requested type.
//!   Panics if component doesn't provide the requested type.
//!   Skips entities without the component.
//!
//! ### Systems
//!
//! Systems is convenient way to build logic that operates on [`World`].
//! Edict defines [`System`] trait to run logic on [`World`] and [`IntoSystem`] for types convertible to [`System`].
//!
//! Functions may implement [`IntoSystem`] automatically -
//! it is required to return `()` and accept arguments that implement [`FnArg`] trait.
//! There are [`FnArg`] implementations for [`View`]s to iterate over entities,
//! [`Res`] and [`ResMut`], [`ResNoSync`] and [`ResMutNoSend`] to access resources,
//! [`ActionEncoder`] to record actions that mutate [`World`]'s state and [`State`] to store system's local state between runs.
//!
//! ### Easy scheduler
//!
//! [`Scheduler`] is provided to run [`System`]s.
//! Systems added to the [`Scheduler`] run in parallel where possible,
//! however they act **as if** executed sequentially in order they were added.
//!
//! If systems do not conflict they may be executed in parallel.
//!
//! If systems conflict, the one added first will be executed before the one added later can start.
//!
//! `std` threads or `rayon` can be used as an executor.
//! User may provide custom executor by implementing [`ScopedExecutor`] trait.
//!
//! Requires `"scheduler"` feature which is enabled by default.
//!
//! ### Async
//!
//! Futures executor to run logic that requires waiting for certain conditions or events
//! or otherwise spans for multiple ticks.
//!
//! Logic that requires waiting can be complex to implement using systems.
//! Systems run in loop and usually work on every entity with certain components.
//! Implementing waiting logic would require adding waiting state to existing or new components and
//! logic would be spread across many system runs or even many systems.
//!
//! Futures may use `await` syntax to wait for certain conditions or events.
//! Futures that can access ECS data are referred in Edict as "flows".
//!
//! Flows can be spawned in the [`World`] using [`World::spawn_flow`] method.
//! [`Flows`] type is used as an executor to run spawned flows.
//!
//! Flows can be bound to an entity and spawned using [`World::spawn_flow_for`] method, [`EntityRef::spawn_flow`] or [`flow::Entity::spawn_flow`]
//! Such flows will be cancelled if entity is despawned.
//!
//! Due to borrow checker limitations, closures can't be spawned as flows directly,
//! To work around this issue [`flow_fn!`] macro accepts valid closure syntax and produces a flow that can be spawned.
//!
//! User may implement low-level futures using `poll*` methods of [`flow::World`] and [`flow::Entity`] to access tasks [`Context`].
//! Edict provides only a couple of low-level futures that will do the waiting:
//! [`yield_now!`] yields control to the executor once and resumes on next execution.
//!
//! It is recommended to use flows for high-level logic that spans multiple ticks
//! and use systems to do low-level logic that runs every tick.
//! Flows may request systems to perform operations by adding special components to entities.
//! And systems may spawn flows to do long-running operations.
//!
//! Requires `"flow"` feature which is enabled by default.
//!
//! # no_std support
//!
//! Edict can be used in `no_std` environment but requires `alloc` crate.
//! `"std"` feature is enabled by default.
//!
//! If "std" feature is disabled, error types will not implement [`std::error::Error`].
//! And "flow" and "scheduler" feature would require extern functions to be provided.
//!
//! [`!Send`]: core::marker::Send
//! [`!Sized`]: core::marker::Sized
//! [`!Sync`]: core::marker::Sync
//! [`ActionEncoder`]: crate::action::ActionEncoder
//! [`AliveEntity`]: crate::entity::AliveEntity
//! [`BorrowAll`]: crate::query::BorrowAll
//! [`BorrowAny`]: crate::query::BorrowAny
//! [`BorrowOne`]: crate::query::BorrowOne
//! [`Component`]: crate::component::Component
//! [`Component::on_drop`]: crate::component::Component::on_drop
//! [`Component::on_replace`]: crate::component::Component::on_replace
//! [`Context`]: std::task::Context
//! [`Entity`]: crate::entity::Entity
//! [`EntityBound`]: crate::entity::EntityBound
//! [`EntityId`]: crate::entity::EntityId
//! [`EntityLoc`]: crate::entity::EntityLoc
//! [`EntityRef`]: crate::entity::EntityRef
//! [`EntityRef::spawn_flow`]: crate::entity::EntityRef::spawn_flow
//! [`flow`]: crate::flow
//! [`flow_fn!`]: crate::flow::flow_fn
//! [`Flows`]: crate::flow::Flows
//! [`Flows::execute`]: crate::flow::Flows::execute
//! [`FnArg`]: crate::system::FnArg
//! [`IdRange`]: crate::entity::IdRange
//! [`IdRangeAllocator`]: crate::entity::IdRangeAllocator
//! [`IntoSystem`]: crate::system::IntoSystem
//! [`LocalActionEncoder`]: crate::action::LocalActionEncoder
//! [`Modified`]: crate::query::Modified
//! [`Query`]: crate::query::Query
//! [`Relation`]: crate::relation::Relation
//! [`Res`]: crate::resources::Res
//! [`ResMut`]: crate::resources::ResMut
//! [`ResNoSync`]: crate::system::ResNoSync
//! [`ResMutNoSend`]: crate::system::ResMutNoSend
//! [`Scheduler`]: crate::scheduler::Scheduler
//! [`ScopedExecutor`]: crate::executor::ScopedExecutor
//! [`State`]: crate::system::State
//! [`System`]: crate::system::System
//! [`TypeId`]: core::any::TypeId
//! [`View`]: crate::view::View
//! [`ViewCell`]: crate::view::ViewCell
//! [`World`]: crate::world::World
//! [`World::epoch`]: crate::world::World::epoch
//! [`World::spawn_flow`]: crate::world::World::spawn_flow
//! [`World::spawn_flow_for`]: crate::world::World::spawn_flow_for
//! [`WorldBuilder`]: crate::world::WorldBuilder
//! [`WorldLocal`]: crate::world::WorldLocal
//! [`WorldLocal::defer*`]: crate::world::WorldLocal::defer
//! [`WorldShare`]: crate::world::WorldShare
//!

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(unused_must_use)]
#![allow(unused_unsafe)]

extern crate alloc;
extern crate self as edict;

use core::{any::TypeId, fmt};

macro_rules! indexed_tuple {
    ($idx:ident => $($e:expr),* $(,)?) => {{
        let mut $idx = 0;
        ($({
            let e = $e;
            $idx += 1;
            e
        },)*)
    }};
}

macro_rules! for_tuple {
    ($macro:ident) => {
        for_tuple!($macro for A B C D E F G H I J K L M N O P);
    };
    ($macro:ident for ) => {
        $macro!();
    };
    ($macro:ident for $head:ident $($tail:ident)*) => {
        for_tuple!($macro for $($tail)*);
        $macro!($head $($tail)*);
    };
}

#[cfg(feature = "alkahest")]
macro_rules! for_tuple_2 {
    ($macro:ident) => {
        for_tuple_2!($macro for
            AA AB AC AD AE AF AG AH AI AJ AK AL AM AN AO AP,
            BA BB BC BD BE BF BG BH BI BJ BK BL BM BN BO BP
        );
    };
    ($macro:ident for ,) => {
        $macro!(,);
    };
    ($macro:ident for $a_head:ident $($a_tail:ident)*, $b_head:ident $($b_tail:ident)*) => {
        for_tuple_2!($macro for $($a_tail)*, $($b_tail)*);

        $macro!($a_head $($a_tail)*, $b_head $($b_tail)*);
    };
}

macro_rules! impl_copy {
    ($type:ident $(< $( $a:ident ),+ >)? $(where $($b:path: $b0:ident $(+ $bt:ident)*),+ $(,)?)?) => {
        impl $(< $( $a ),+ >)? Copy for $type $(< $( $a ),+ >)?
        $(where $($b: $b0 $(+$bt)*,)+)?
        {}

        impl $(< $( $a ),+ >)? Clone for $type $(< $( $a ),+ >)?
        $(where $($b: $b0 $(+$bt)*,)+)?
        {
            #[inline(always)]
            fn clone(&self) -> Self {
                *self
            }

            #[inline(always)]
            fn clone_from(&mut self, source: &Self) {
                *self = *source
            }
        }
    };
}

macro_rules! impl_debug {
    ($type:ident $(< $( $a:ident ),+ >)? { $($fname:ident)* } $(where $($b:path: $b0:ident $(+ $bt:ident)*),+ $(,)?)?) => {
        impl $(< $( $a ),+ >)? core::fmt::Debug for $type $(< $( $a ),+ >)?
        $(where $($b: $b0 $(+$bt)*,)+)?
        {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                f.debug_struct(stringify!($type))
                    $(.field(stringify!($fname), &self.$fname))*
                    .finish()
            }
        }
    };
}

macro_rules! marker_type_impls {
    (
        $(#[$meta:meta])*
        $vis:vis struct $type:ident $(< $($a:ident),+ >)?;
    ) => {
        impl $(< $($a: ?Sized),+ >)? $type $(< $($a),+ >)? {
            /// Constructs new marker instance.
            /// This function is noop as it returns ZST and have no side-effects.
            #[must_use]
            pub const fn new() -> Self {
                $type
            }
        }

        impl $(< $($a: ?Sized),+ >)? core::marker::Copy for $type $(< $($a),+ >)? {}
        impl $(< $($a: ?Sized),+ >)? core::clone::Clone for $type $(< $($a),+ >)? {
            #[inline(always)]
            fn clone(&self) -> Self {
                $type
            }

            #[inline(always)]
            fn clone_from(&mut self, _source: &Self) {}
        }

        impl $(< $($a: ?Sized),+ >)? core::fmt::Debug for $type $(< $($a),+ >)?
        $(where $($a : 'static,)+)?
        {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, core::stringify!($type))?;
                $(
                    write!(f, "<")?;
                    $(write!(f, "{}", core::any::type_name::<$a>())?;)+
                    write!(f, ">")?;
                )?
                Ok(())
            }
        }

        impl $(< $($a: ?Sized),+ >)? Default for $type $(< $($a),+ >)? {
            #[inline(always)]
            fn default() -> Self {
                $type
            }
        }
    };
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[doc(hidden)]
pub enum MarkerVoid {}

#[doc(hidden)]
pub struct TypeParam<T: ?Sized>([*const T; 0]);

unsafe impl<T: ?Sized> Send for TypeParam<T> {}
unsafe impl<T: ?Sized> Sync for TypeParam<T> {}

impl<T: ?Sized> Copy for TypeParam<T> {}
impl<T: ?Sized> Clone for TypeParam<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        TypeParam([])
    }

    #[inline(always)]
    fn clone_from(&mut self, _source: &Self) {}
}

macro_rules! marker_type {
    (
        $(#[$meta:meta])*
        $vis:vis struct $type:ident;
    ) => {
        $(#[$meta])*
        $vis struct $type;

        marker_type_impls!{
            $(#[$meta])*
            $vis struct $type;
        }
    };

    (
        $(#[$meta:meta])*
        $vis:vis struct $type:ident < $($a:ident),+ >;
    ) => {
        $(#[$meta])*
        $vis enum $type < $($a: ?Sized,)+ > {
            /// Instance of this type.
            $type,
            #[doc(hidden)]
            __Void($crate::MarkerVoid, $($crate::TypeParam<$a>,)+),
        }

        /// Imports $type as value symbol.
        pub use self::$type::*;

        marker_type_impls!{
            $(#[$meta])*
            $vis struct $type < $($a),+ >;
        }
    };
}

mod hash;

pub mod action;
pub mod archetype;
pub mod bundle;
pub mod component;
pub mod dump;
pub mod entity;
pub mod epoch;
pub mod executor;
pub mod query;
pub mod relation;
pub mod resources;
pub mod system;
pub mod view;
pub mod world;

#[cfg(feature = "flow")]
pub mod flow;

#[cfg(feature = "scheduler")]
pub mod scheduler;

#[cfg(not(feature = "std"))]
pub mod nostd;

#[cfg(test)]
mod test;

// pub mod prelude;

/// Error that may be returned when an entity is not found in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoSuchEntity;

impl fmt::Display for NoSuchEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Specified entity is not found")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NoSuchEntity {}

/// Error that may be returned when an entity does not have required components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mismatch;

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Entity does not match requirements")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Mismatch {}

/// Error that may be returned when fetching query from entity that
/// may be despawned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityError {
    /// Error returned when an entity is not found in the world.
    NoSuchEntity,

    /// Entity alive but does not match requirements of the operations.
    /// Typicall it means that required component is missing.
    Mismatch,
}

unsafe trait ResultEntityError<T> {
    unsafe fn assume_entity_exists(self) -> Option<T>;
}

unsafe impl<T> ResultEntityError<T> for Result<T, EntityError> {
    #[inline(always)]
    unsafe fn assume_entity_exists(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(EntityError::Mismatch) => None,
            Err(EntityError::NoSuchEntity) => unsafe { core::hint::unreachable_unchecked() },
        }
    }
}

impl From<NoSuchEntity> for EntityError {
    #[inline(always)]
    fn from(_: NoSuchEntity) -> Self {
        EntityError::NoSuchEntity
    }
}

impl From<Mismatch> for EntityError {
    #[inline(always)]
    fn from(_: Mismatch) -> Self {
        EntityError::Mismatch
    }
}

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntityError::NoSuchEntity => fmt::Display::fmt(&NoSuchEntity, f),
            EntityError::Mismatch => fmt::Display::fmt(&Mismatch, f),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EntityError {}

/// Specifies kind of access query performs for particular component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    /// Shared access to component. Can be aliased with other [`Access::Read`] accesses.
    Read,

    /// Cannot be aliased with any other access.
    Write,
}

#[doc(hidden)]
pub mod private {
    pub use alloc::{sync::Arc, vec::Vec};
    pub use core::{
        any::Any,
        marker::{PhantomData, Send, Sync},
        mem::MaybeUninit,
        option::Option,
        ptr::NonNull,
    };

    use crate::system::{IntoSystem, IsFunctionSystem};

    pub use crate::system::FnArg;

    #[inline(always)]
    pub fn is_fn_arg<A: FnArg>() {}

    #[inline(always)]
    pub fn is_fn_system<Args, F: IntoSystem<IsFunctionSystem<Args>>>(_: F) {}
}

#[doc(hidden)]
pub struct ExampleComponent;

impl component::Component for ExampleComponent {}

#[cold]
#[inline(always)]
fn cold() {}

/// Shorter version of [`core::any::TypeId::of`].
fn type_id<T: 'static + ?Sized>() -> TypeId {
    TypeId::of::<T>()
}
