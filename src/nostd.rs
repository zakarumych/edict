//! This module contains submodules with extern function declarations.
//!
//! It is only relevant when "std" feature is disabled.
//!
//! When submodule's feature is enabled, it is required to supply implementation for each function.
//! Otherwise linker error may occur.

/// Declares extern functions required when "flow" feature is enabled without "std".
#[cfg(feature = "flow")]
pub mod flow {
    use crate::entity::EntityId;
    use core::ptr::NonNull;

    extern "C" {
        pub fn edict_set_flow_world_tls(world: NonNull<u8>) -> Option<NonNull<u8>>;
        pub fn edict_get_flow_world_tls() -> Option<NonNull<u8>>;
        pub fn edict_reset_flow_world_tls(prev: Option<NonNull<u8>>, world: NonNull<u8>);
        pub fn edict_set_flow_entity_tls(entity: EntityId) -> Option<EntityId>;
        pub fn edict_get_flow_entity_tls() -> Option<EntityId>;
        pub fn edict_reset_flow_entity_tls(prev: Option<EntityId>, entity: EntityId);
    }
}

/// Declares extern functions required when "scheduler" feature is enabled without "std".
#[cfg(feature = "scheduler")]
pub mod scheduler {
    extern "C" {
        /// Returns the current thread opaque handle.
        pub fn edict_current_thread() -> *mut u8;

        /// Parks the current thread.
        pub fn edict_park_thread();

        /// Unparks the thread.
        pub fn edict_unpark_thread(thread: *mut u8);
    }
}
