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

extern crate alloc;
extern crate self as edict;

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
    ($type:ident < $( $a:ident ),+ >) => {
        impl< $( $a ),+ > Copy for $type < $( $a ),+ > {}
        impl< $( $a ),+ > Clone for $type < $( $a ),+ > {
            fn clone(&self) -> Self {
                *self
            }
        }
    };
}

macro_rules! impl_debug {
    ($type:ident < $( $a:ident ),+ > { $($fname:ident)* }) => {
        impl< $( $a ),+ > core::fmt::Debug for $type < $( $a ),+ > {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                f.debug_struct(stringify!($type))
                    $(.field(stringify!($fname), &self.$fname))*
                    .finish()
            }
        }
    };
}

macro_rules! phantom_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis struct $type:ident < $a:ident >
    ) => {
        $(#[$meta])*
        $vis struct $type < $a > {
            marker: core::marker::PhantomData< $a >,
        }

        impl< $a > $type < $a > {
            /// Constructs new phantom wrapper instance.
            /// This function is noop as it returns ZST and have no side-effects.
            #[must_use]
            pub const fn new() -> Self {
                $type {
                    marker: core::marker::PhantomData,
                }
            }
        }

        impl_copy!($type < $a >);

        impl< $a > core::fmt::Debug for $type < $a >
        where
            $a : 'static
        {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, core::concat!(core::stringify!($type), "<{}>"), core::any::type_name::<$a>())
            }
        }

        impl< $a > Default for $type < $a > {
            fn default() -> Self {
                Self::new()
            }
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
// pub mod prelude;
pub mod query;
pub mod relation;
pub mod scheduler;
pub mod system;
pub mod task;
pub mod world;

mod hash;
mod idx;
mod res;

#[cfg(test)]
mod test;

#[doc(hidden)]
pub mod private {
    pub use alloc::vec::Vec;
}

#[doc(hidden)]
pub struct ExampleComponent;

impl component::Component for ExampleComponent {}

// #[doc(inline)]
// pub use self::prelude::*;

#[cold]
#[inline(always)]
fn cold() {}
