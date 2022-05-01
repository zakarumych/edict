//! Type aliases for world epoch and component version.
//! This aliases is created to easy adjust the epoch type size.

// TODO use usize for Epoch as soon as overflow wrapping of u32 is safe and tested

/// Type for world's epoch and component version.
pub type Epoch = u64;
/// Atomic type for world's epoch and component version.
pub type AtomicEpoch = core::sync::atomic::AtomicU64;
