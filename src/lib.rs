//!
//! ## Edict
//!
//! Edict is a fast and powerful ECS crate that expands traditional ECS feature set.
//! Written in Rust by your fellow 🦀
//!
//! ### Features
//!
//! * General purpose archetype based ECS with fast iteration.
//!
//! * Relations can be added to pair of entities, binding them together.
//!   When either of the two entities is despawned, relation is dropped.
//!   [`Relation`] type may further configure behavior of the bonds.
//!
//! * Change tracking.
//!   Each component instance is equipped with epoch counter that tracks last potential mutation of the component.
//!   Special query type uses epoch counter to skip entities where component wasn't changed since specified epoch.
//!   Last epoch can be obtained with [`World::epoch`].
//!
//! * Built-in type-map for singleton values called "resources".
//!   Resources can be inserted into/fetched from [`World`].
//!   Resources live separately from entities and their components.
//!
//! * Runtime checks for query validity and mutable aliasing avoidance.
//!   This requires atomic operations at the beginning iteration on next archetype.
//!
//! * Support for [`!Send`] and [`!Sync`] components.
//!   [`!Send`] components cannot be fetched mutably from outside "main" thread.
//!   [`!Sync`] components cannot be fetched immutably from outside "main" thread.
//!   [`World`] has to be [`!Send`] but implements [`Sync`].
//!
//! * [`ActionEncoder`] allows recording actions and later run them on [`World`].
//!   Actions get mutable access to [`World`].
//!
//! * Component replace/drop hooks.
//!   Components can define hooks that will be executed on value drop and replace.
//!   Hooks can read old and new values, [`EntityId`] and can record actions into [`ActionEncoder`].
//!
//! * Component type may define a set of types that can be borrowed from it.
//!   Borrowed type may be not sized, allowing slices, dyn traits and any other [`!Sized`] types.
//!   There's macro to define dyn trait borrows.
//!   Special kind of queries look into possible borrows to fetch.
//!
//! * [`WorldBuilder`] can be used to manually register component types and override default behavior.
//!
//! * Optional [`Component`] trait to allow implicit component type registration by insertion methods.
//!   Implicit registration uses behavior defined by [`Component`] implementation as-is.
//!   Separate insertions methods with [`Component`] trait bound lifted can be used where trait is not implemented or implementation is not visible for generic type.
//!   Those methods require pre-registration of the component type. If type was not registered - method panics.
//!   Both explicit registration with [`WorldBuilder`] and implicit registration via insertion method with [`Component`] type bound is enough.
//!
//! * [`System`] trait and [`IntoSystem`] implemented for functions if argument types implement [`FnArg`].
//!   This way practically any system can be defined as a function.
//!
//! * [`Scheduler`] that can run [`System`]s in parallel using provided executor.
//!
//! [`Send`]: core::marker::Send
//! [`!Send`]: core::marker::Send
//! [`Sync`]: core::marker::Sync
//! [`!Sync`]: core::marker::Sync
//! [`World`]: edict::world::World
//! [`WorldBuilder`]: edict::world::WorldBuilder
//! [`ActionEncoder`]: edict::action::ActionEncoder
//! [`EntityId`]: edict::entity::EntityId
//! [`!Sized`]: core::marker::Sized
//! [`Component`]: edict::component::Component
//! [`World::epoch`]: edict::world::World::epoch
//! [`Relation`]: edict::relation::Relation
//! [`System`]: edict::system::System
//! [`IntoSystem`]: edict::system::IntoSystem
//! [`FnArg`]: edict::system::FnArg
//! [`Scheduler`]: edict::scheduler::Scheduler

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
// #![deny(unsafe_op_in_unsafe_fn)]
#![allow(unused_unsafe)]

extern crate alloc;
extern crate self as edict;

use core::{any::TypeId, fmt};

pub use atomicell;

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
        impl $(< $($a),+ >)? $type $(< $($a),+ >)? {
            /// Constructs new marker instance.
            /// This function is noop as it returns ZST and have no side-effects.
            #[must_use]
            pub const fn new() -> Self {
                $type
            }
        }

        impl $(< $($a),+ >)? core::marker::Copy for $type $(< $($a),+ >)? {}
        impl $(< $($a),+ >)? core::clone::Clone for $type $(< $($a),+ >)? {
            #[inline(always)]
            fn clone(&self) -> Self {
                $type
            }

            #[inline(always)]
            fn clone_from(&mut self, _source: &Self) {}
        }

        impl $(< $($a),+ >)? core::fmt::Debug for $type $(< $($a),+ >)?
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

        impl $(< $($a),+ >)? Default for $type $(< $($a),+ >)? {
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

/// Extends output with `ComponentBorrow` to borrow dyn trait object.
/// `dyn Trait + Send + Sync` and all valid combinations are automatically added.
#[macro_export]
macro_rules! borrow_dyn_trait {
    ($self:ident as $trait:path => $extend:ident) => {{
        #![allow(dead_code)]

        struct DispatchBorrowSendSync<T>(DispatchBorrowSend<T>);
        struct DispatchBorrowSend<T>(DispatchBorrowSync<T>);
        struct DispatchBorrowSync<T>(DispatchBorrow<T>);
        struct DispatchBorrow<T>(core::marker::PhantomData<T>);

        impl<T> core::ops::Deref for DispatchBorrowSendSync<T> {
            type Target = DispatchBorrowSend<T>;

            fn deref(&self) -> &DispatchBorrowSend<T> {
                &self.0
            }
        }

        impl<T> core::ops::Deref for DispatchBorrowSend<T> {
            type Target = DispatchBorrowSync<T>;

            fn deref(&self) -> &DispatchBorrowSync<T> {
                &self.0
            }
        }

        impl<T> core::ops::Deref for DispatchBorrowSync<T> {
            type Target = DispatchBorrow<T>;

            fn deref(&self) -> &DispatchBorrow<T> {
                &self.0
            }
        }

        impl<T: $trait + Send + Sync + 'static> DispatchBorrowSendSync<T> {
            fn insert(
                &self,
                extend: &mut impl core::iter::Extend<$crate::component::ComponentBorrow>,
            ) {
                self.insert_one(extend);
                self.0.insert_one(extend);
                self.0 .0.insert_one(extend);
                self.0 .0 .0.insert_one(extend);
            }

            fn insert_one(
                &self,
                extend: &mut impl core::iter::Extend<$crate::component::ComponentBorrow>,
            ) {
                extend.extend(Some($crate::component::ComponentBorrow::make(
                    |ptr: core::ptr::NonNull<u8>,
                     core::marker::PhantomData|
                     -> &(dyn $trait + Send + Sync) {
                        unsafe { ptr.cast::<T>().as_ref() }
                    },
                    core::option::Option::Some(
                        |ptr: core::ptr::NonNull<u8>,
                         core::marker::PhantomData|
                         -> &mut (dyn $trait + Send + Sync) {
                            unsafe { ptr.cast::<T>().as_mut() }
                        },
                    ),
                )));
            }
        }

        impl<T: $trait + Send + 'static> DispatchBorrowSend<T> {
            fn insert(
                &self,
                extend: &mut impl core::iter::Extend<$crate::component::ComponentBorrow>,
            ) {
                self.insert_one(extend);
                self.0 .0.insert_one(extend);
            }

            fn insert_one(
                &self,
                extend: &mut impl core::iter::Extend<$crate::component::ComponentBorrow>,
            ) {
                extend.extend(Some($crate::component::ComponentBorrow::make(
                    |ptr: core::ptr::NonNull<u8>,
                     core::marker::PhantomData|
                     -> &(dyn $trait + Send) {
                        unsafe { ptr.cast::<T>().as_ref() }
                    },
                    core::option::Option::Some(
                        |ptr: core::ptr::NonNull<u8>,
                         core::marker::PhantomData|
                         -> &mut (dyn $trait + Send) {
                            unsafe { ptr.cast::<T>().as_mut() }
                        },
                    ),
                )));
            }
        }

        impl<T: $trait + Sync + 'static> DispatchBorrowSync<T> {
            fn insert(
                &self,
                extend: &mut impl core::iter::Extend<$crate::component::ComponentBorrow>,
            ) {
                self.insert_one(extend);
                self.0.insert_one(extend);
            }

            fn insert_one(
                &self,
                extend: &mut impl core::iter::Extend<$crate::component::ComponentBorrow>,
            ) {
                extend.extend(Some($crate::component::ComponentBorrow::make(
                    |ptr: core::ptr::NonNull<u8>,
                     core::marker::PhantomData|
                     -> &(dyn $trait + Sync) {
                        unsafe { ptr.cast::<T>().as_ref() }
                    },
                    core::option::Option::Some(
                        |ptr: core::ptr::NonNull<u8>,
                         core::marker::PhantomData|
                         -> &mut (dyn $trait + Sync) {
                            unsafe { ptr.cast::<T>().as_mut() }
                        },
                    ),
                )));
            }
        }

        impl<T: $trait + 'static> DispatchBorrow<T> {
            fn insert(
                &self,
                extend: &mut impl core::iter::Extend<$crate::component::ComponentBorrow>,
            ) {
                self.insert_one(extend);
            }

            fn insert_one(
                &self,
                extend: &mut impl core::iter::Extend<$crate::component::ComponentBorrow>,
            ) {
                extend.extend(Some($crate::component::ComponentBorrow::make(
                    |ptr: core::ptr::NonNull<u8>, core::marker::PhantomData| -> &dyn $trait {
                        unsafe { ptr.cast::<T>().as_ref() }
                    },
                    core::option::Option::Some(
                        |ptr: core::ptr::NonNull<u8>,
                         core::marker::PhantomData|
                         -> &mut dyn $trait {
                            unsafe { ptr.cast::<T>().as_mut() }
                        },
                    ),
                )));
            }
        }

        let dispatch = DispatchBorrowSendSync(DispatchBorrowSend(DispatchBorrowSync(
            DispatchBorrow(core::marker::PhantomData::<$self>),
        )));
        dispatch.insert(&mut $extend);
    }};
}

/// Constructs `ComponentBorrow` to borrow dyn trait object.
#[macro_export]
macro_rules! borrow_dyn_any {
    ($self:ident => $extend:ident) => {{
        $crate::borrow_dyn_trait!($self as core::any::Any => $extend)
    }};
}

pub mod action;
pub mod archetype;
pub mod bundle;
pub mod component;
pub mod dump;
pub mod entity;
pub mod epoch;
pub mod executor;
pub mod prelude;
pub mod query;
pub mod relation;
pub mod system;
pub mod view;
pub mod world;

#[cfg(feature = "std")]
pub mod scheduler;
#[cfg(feature = "std")]
pub mod task;

mod hash;
mod idx;
mod res;

#[cfg(test)]
mod test;

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

/// Error that may be returned when fetching query from entity that
/// may be despawned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityError {
    /// Error returned when an entity is not found in the world.
    NoSuchEntity,

    /// Entity alive but does not match query.
    QueryMismatch,
}

unsafe trait ResultEntityError<T> {
    unsafe fn assume_entity_exists(self) -> Option<T>;
}

unsafe impl<T> ResultEntityError<T> for Result<T, EntityError> {
    #[inline(always)]
    unsafe fn assume_entity_exists(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(EntityError::QueryMismatch) => None,
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

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntityError::NoSuchEntity => f.write_str("Specified entity is not found"),
            EntityError::QueryMismatch => f.write_str("Entity does not match query"),
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

impl Access {
    #[inline(always)]
    fn read_type<T: 'static>(ty: TypeId) -> Option<Self> {
        if ty == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline(always)]
    fn write_type<T: 'static>(ty: TypeId) -> Option<Self> {
        if ty == TypeId::of::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }
}

#[doc(hidden)]
pub mod private {
    pub use alloc::vec::Vec;
}

#[doc(hidden)]
pub struct ExampleComponent;

impl component::Component for ExampleComponent {}

#[doc(inline)]
pub use self::prelude::*;

#[cold]
#[inline(always)]
fn cold() {}
