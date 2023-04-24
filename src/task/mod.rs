//! This module provides mechanism for storing async tasks in ECS, potentially attaching them to existing entities.
//! Tasks are executed by [`task_system`].

/// Data that is passed to the [`RawWakerVTable`] functions.
struct RawWakerData<T> {
    /// [`Task`]s [`EntityId`].
    entity: EntityId,
    task_marker: PhantomData<fn() -> T>,
}


struct 