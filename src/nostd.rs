//! This module contains submodules with extern function declarations.
//!
//! It is only relevant when "std" feature is disabled.
//!
//! When submodule's feature is enabled, it is required to supply implementation for each function.
//! Otherwise linker error may occur.

/// Declares extern functions required when "flow" feature is enabled without "std".
#[cfg(feature = "flow")]
pub mod flow {
    use core::ptr::NonNull;

    unsafe extern "C" {
        /// Sets the current world pointer in thread-local storage.
        /// Returns previous pointer if any.
        ///
        /// # Safety
        ///
        /// Pointer to the world will be stored in thread-local storage
        /// and returned by [`edict_get_flow_world_tls`].
        /// At which point it can be accessed mutably, thus world passed to this function
        /// should not be accessed otherwise.
        ///
        /// Up until the thread-local storage is reset using [`edict_reset_flow_world_tls`].
        pub unsafe fn edict_set_flow_world_tls(world: NonNull<u8>) -> Option<NonNull<u8>>;

        /// Returns the current world pointer in thread-local storage.
        ///
        /// # Safety
        ///
        /// This function should be safe to call.
        ///
        /// Returned pointer must remain valid until [`edict_reset_flow_world_tls`] is called to reset it.
        pub unsafe fn edict_get_flow_world_tls() -> Option<NonNull<u8>>;

        /// Resets the current world pointer in thread-local storage.
        ///
        /// # Safety
        ///
        /// This function should be safe to call if the following conditions are met:
        ///
        /// - The `prev` pointer is the value returned by `edict_set_flow_world_tls` when the `world` was passed in on this thread.
        /// - The `world` pointer must be the current world pointer in thread-local storage.
        ///   i.e. for each call to [`edict_reset_flow_world_tls`] made after the call that set the `world` pointer,
        ///   [`edict_reset_flow_world_tls`] call was made.
        pub unsafe fn edict_reset_flow_world_tls(prev: Option<NonNull<u8>>, world: NonNull<u8>);
    }
}
