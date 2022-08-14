//! Built-in scheduling for systems.
//!
//! Users are free to use their own scheduling.
//!
//! Built-in [`Scheduler`] has following properties:
//! * Separates execution of conflicting systems temporally.
//! * Executes non-conflicting systems in parallel on available worker threads.
//! * Conflicting systems are executed in order of their registration.
//!   That means that system defines implicit dependency on all systems with with there's conflict.
//!   * In case of write-to-read conflict, reading system that is added later is guaranteed
//!     to observe modifications made by writing system that was added before.
//!   * In case of read-to-write conflict, reading system that is added before is guaranteed
//!     to NOT observe modifications made by writing system that was added later.
//!   * In case of write-to-write conflict, writing system that is added before is guaranteed
//!     to NOT observe modifications made by writing system that was added later.
//!     And writing system that is added later is guaranteed
//!     to observe modifications made by writing system that was added before.
//!

/// System trait that should be implemented by systems in order to be added to the [`Scheduler`].
/// Systems run with context `C` that is shared between all systems.
pub trait System<C> {}
