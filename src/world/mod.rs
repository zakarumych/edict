//! Self-contained ECS [`World`].
//!
//! There's 3 kinds of `World`s which are wrap one another:
//! * [`World`]         - non-shareable world that is pinned to a thread where it is accessed.
//! * [`WorldShare`]    - shareable world. Derefs to `World`.
//! * [`WorldLocal`]    - World that is known to be on the "main" thread. Derefs for `World`.
//!

use alloc::{vec, vec::Vec};
use core::{
    any::type_name,
    cell::UnsafeCell,
    convert::TryFrom,
    fmt::{self, Debug},
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    action::{ActionChannel, ActionSender, LocalActionBuffer, LocalActionEncoder},
    archetype::Archetype,
    bundle::{BundleDesc, ComponentBundleDesc},
    component::{Component, ComponentInfo, ComponentRegistry},
    entity::{AliveEntity, Entity, EntityId, EntityLoc, EntityRef, EntitySet},
    epoch::{EpochCounter, EpochId},
    resources::Resources,
    type_id, NoSuchEntity,
};

use self::edges::Edges;

pub(crate) use self::spawn::iter_reserve_hint;

pub use self::builder::WorldBuilder;

// /// Takes internal action buffer and
// /// runs expression with it.
// /// After expression completes, action buffer is executed
// /// and then returned to the world.
// ///
// /// While action buffer is taken,
// /// all paths would either use world's methods with explicit buffer
// /// or use `with_buffer` method,
// /// ensuring that action buffer is always present
// /// when this macro is called.
// macro_rules! with_buffer {
//     ($world:ident, $buffer:ident => $expr:expr) => {
//         unsafe {
//             let mut buffer = $world.action_buffer.take().unwrap_unchecked();
//             let result = {
//                 let $buffer = &mut buffer;
//                 $expr
//             };
//             if $world.execute_action_buffer && !buffer.is_empty() {
//                 ActionBuffer::execute(&mut buffer, $world);
//             }
//             $world.action_buffer = Some(buffer);
//             result
//         }
//     };
// }

mod builder;
mod edges;
mod get;
mod insert;
mod relation;
mod remove;
mod resource;
mod spawn;
mod view;

/// Unique id for the archetype set.
/// Same sets may or may not share id, but different sets never share id.
/// `World` keeps same id until archetype set changes.
///
/// This value starts with 1 because 0 is reserved for empty set.
static NEXT_ARCHETYPE_SET_ID: AtomicU64 = AtomicU64::new(1);

struct ArchetypeSet {
    /// Unique archetype set id.
    /// Changes each time new archetype is added.
    id: u64,

    archetypes: Vec<Archetype>,
}

impl Deref for ArchetypeSet {
    type Target = [Archetype];

    fn deref(&self) -> &[Archetype] {
        &self.archetypes
    }
}

impl DerefMut for ArchetypeSet {
    fn deref_mut(&mut self) -> &mut [Archetype] {
        &mut self.archetypes
    }
}

impl ArchetypeSet {
    fn new() -> Self {
        let null_archetype = Archetype::new(core::iter::empty());
        ArchetypeSet {
            // All archetype sets starts the same.
            id: 0,
            archetypes: vec![null_archetype],
        }
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn add_with(&mut self, f: impl FnOnce(&[Archetype]) -> Archetype) -> u32 {
        let len = match u32::try_from(self.archetypes.len()) {
            Ok(u32::MAX) | Err(_) => panic!("Too many archetypes"),
            Ok(len) => len,
        };
        let new_archetype = f(&self.archetypes);
        self.archetypes.push(new_archetype);

        // Update archetype set id to new process-wide unique value.
        // Assume u64 increment won't overflow.
        self.id = NEXT_ARCHETYPE_SET_ID.fetch_add(1, Ordering::Relaxed);
        len
    }
}

/// Container for entities with any sets of components.
///
/// Entities can be spawned in the [`World`] with handle [`EntityId`] returned,
/// that can be used later to access that entity.
///
/// [`EntityId`] handle can be downgraded to [`EntityId`].
///
/// Entity would be despawned after last [`EntityId`] is dropped.
///
/// Entity's set of components may be modified in any way.
///
/// Entities can be fetched directly, using [`EntityId`] or [`EntityId`]
/// with different guarantees and requirements.
///
/// Entities can be efficiently queried from `World` to iterate over all entities
/// that match query requirements.
///
/// Internally [`World`] manages entities generations,
/// maps entity to location of components in archetypes,
/// moves components of entities between archetypes,
/// spawns and despawns entities.
///
/// [`World`] type is not `Sync` or `Send`, but an instance is allowed to be shared
/// from [`WorldShare`] references.
/// However mutable reference [`World`]
///
/// ```compile_fail
/// # use edict::world::World;
///
/// fn is_send<T: core::marker::Send>() {}
/// is_send::<World>();
/// ```
///
/// ```compile_fail
/// # use edict::world::World;
///
/// fn is_sync<T: core::marker::Sync>() {}
/// is_sync::<World>();
/// ```
pub struct World {
    /// Epoch counter of the World.
    /// Incremented on each mutable query.
    epoch: EpochCounter,

    /// Collection of entities with their locations.
    entities: EntitySet,

    /// Archetypes of entities in the world.
    archetypes: ArchetypeSet,

    edges: Edges,

    registry: ComponentRegistry,

    resources: Resources,

    /// Internal action encoder.
    /// This encoder is used to record commands from component hooks.
    /// Commands are immediately executed at the end of the mutating call.
    action_buffer: UnsafeCell<LocalActionBuffer>,

    action_channel: ActionChannel,

    #[cfg(feature = "flow")]
    pub(crate) new_flows: UnsafeCell<crate::flow::NewFlows>,
}

impl Default for World {
    fn default() -> Self {
        World::new()
    }
}

impl Debug for World {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("World").finish_non_exhaustive()
    }
}

impl World {
    /// Returns unique identified of archetype set.
    /// This ID changes each time new archetype is added or removed.
    /// IDs of different worlds are never equal within the same process.
    #[inline(always)]
    pub fn archetype_set_id(&self) -> u64 {
        self.archetypes.id()
    }

    /// Looks up entity location and returns entity with location and bound
    /// to the immutable world borrow, ensuring that entity stays alive
    /// and in the same location.
    #[inline(always)]
    pub fn lookup(&self, entity: impl Entity) -> Result<EntityLoc<'_>, NoSuchEntity> {
        entity.entity_loc(&self.entities).ok_or(NoSuchEntity)
    }

    /// Returns entity reference
    /// that can be used to access entity's components,
    /// insert or remove components, despawn entity etc.
    #[inline(always)]
    pub fn entity(&mut self, entity: impl Entity) -> Result<EntityRef<'_>, NoSuchEntity> {
        self.maintenance();
        entity.entity_ref(self).ok_or(NoSuchEntity)
    }

    /// Returns current world epoch.
    ///
    /// This value can be modified concurrently if [`&World`] is shared.
    /// As it increases monotonically, returned value can be safely assumed as a lower bound.
    ///
    /// [`&World`]: World
    #[inline(always)]
    pub fn epoch(&self) -> EpochId {
        self.epoch.current()
    }

    /// Returns atomic reference to epoch counter.
    #[inline(always)]
    pub fn epoch_counter(&self) -> &EpochCounter {
        &self.epoch
    }

    /// Checks if entity has component of specified type.
    #[inline(always)]
    pub fn has_component<T: 'static>(&self, entity: impl AliveEntity) -> bool {
        let loc = entity.locate(&self.entities);
        if loc.arch == u32::MAX {
            return false;
        }
        self.archetypes[loc.arch as usize].has_component(type_id::<T>())
    }

    /// Checks if entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn try_has_component<T: 'static>(&self, entity: impl Entity) -> Result<bool, NoSuchEntity> {
        let loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;
        if loc.arch == u32::MAX {
            return Ok(false);
        }
        Ok(self.archetypes[loc.arch as usize].has_component(type_id::<T>()))
    }

    /// Checks if entity is alive.
    #[inline(always)]
    pub fn is_alive(&self, id: EntityId) -> bool {
        self.entities.get_location(id).is_some()
    }

    /// Iterate over component info of all registered components
    pub fn iter_component_info(&self) -> impl Iterator<Item = &ComponentInfo> {
        self.registry.iter_info()
    }

    /// Returns a slice of all materialized archetypes.
    pub fn archetypes(&self) -> &[Archetype] {
        &self.archetypes
    }

    /// Returns a slice of all materialized archetypes.
    pub(crate) fn archetypes_mut(&mut self) -> &mut [Archetype] {
        &mut self.archetypes
    }

    /// Returns [`WorldLocal`] referencing this [`World`].
    /// [`WorldLocal`] dereferences to [`World`]
    /// And defines overlapping methods `get_resource` and `get_resource_mut` without `Sync` and `Send` bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// # use core::cell::Cell;
    /// let mut world = World::new();
    /// world.insert_resource(Cell::new(42i32));
    /// let local = world.local();
    /// assert_eq!(42, local.get_resource::<Cell<i32>>().unwrap().get());
    /// ```
    #[inline(always)]
    pub fn local(&mut self) -> &mut WorldLocal {
        WorldLocal::wrap_mut(self)
    }

    /// Returns [`ActionSender`] instance bound to this [`World`].\
    /// [`ActionSender`] can be used to send actions to the [`World`] from
    /// other threads and async tasks.
    ///
    /// [`ActionSender`] API is similar to [`ActionEncoder`](crate::action::ActionEncoder)
    /// except that it can't return [`EntityId`]s of spawned entities.
    ///
    /// To take effect actions must be executed with [`World::execute_received_actions`].
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    ///
    /// let mut world = World::new();
    ///
    /// let action_sender = world.new_action_sender();
    ///
    /// let handle = std::thread::spawn(move || {
    ///    action_sender.closure(|world| {
    ///       world.insert_resource(42i32);
    ///    });
    /// });
    ///
    /// handle.join();
    ///
    /// world.execute_received_actions();
    /// world.expect_resource_mut::<i32>();
    /// ```
    pub fn new_action_sender(&self) -> ActionSender {
        self.action_channel.sender()
    }

    /// Returns [`EntitySet`] from the [`World`].
    pub(crate) fn entities(&self) -> &EntitySet {
        &self.entities
    }

    /// Runs world maintenance.
    ///
    /// Users do not call this method,
    /// it is automatically called in every method that borrows world mutably.
    /// It is one `if zero_check { return }` if no entities were allocated since last call.
    ///
    /// The only observable effect of manual call to this method
    /// is execution of actions encoded with [`ActionSender`].
    #[inline(always)]
    pub(crate) fn maintenance(&mut self) {
        let archetype = &mut self.archetypes[0];
        self.entities
            .spawn_allocated(|id| archetype.spawn_empty(id));
        self.execute_local_actions();
    }

    /// Executes actions received from [`ActionSender`] instances
    /// bound to this [`World`].
    ///
    /// See [`World::new_action_sender`] for more information.
    pub fn execute_received_actions(&mut self) {
        self.maintenance();
        self.action_channel.fetch();
        while let Some(f) = self.action_channel.pop() {
            f.call(self);
        }
    }

    /// Runs world maintenance.
    ///
    /// Users do not call this method,
    /// it is automatically called in every method that borrows world mutably.
    /// It is one `if zero_check { return }` if no entities were allocated since last call.
    ///
    /// The only observable effect of manual call to this method
    /// is execution of actions encoded with [`ActionSender`].
    #[inline(always)]
    fn execute_local_actions(&mut self) {
        while let Some(action) = self.action_buffer.get_mut().pop() {
            action.call(self.local());
        }
    }

    /// Executes deferred actions.
    pub fn run_deferred(&mut self) {
        self.execute_local_actions();
    }
}

/// Wrapper for [`World`] that can be shared between threads.
/// User who intends to share [`World`] between threads should wrap it in [`WorldShare`].
#[repr(transparent)]
pub struct WorldShare {
    inner: World,
}

// WorldMain is only Sync, not Send.
unsafe impl Sync for WorldShare {}

impl Deref for WorldShare {
    type Target = World;

    fn deref(&self) -> &World {
        &self.inner
    }
}

impl DerefMut for WorldShare {
    fn deref_mut(&mut self) -> &mut World {
        &mut self.inner
    }
}

impl Debug for WorldShare {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        <World as Debug>::fmt(&self.inner, f)
    }
}

impl From<WorldShare> for World {
    #[inline]
    fn from(world: WorldShare) -> Self {
        world.inner
    }
}

impl From<World> for WorldShare {
    #[inline]
    fn from(world: World) -> Self {
        WorldShare { inner: world }
    }
}

impl WorldShare {
    /// Creates a new shareable [`World`] instance.
    pub fn new() -> Self {
        WorldShare {
            inner: World::new(),
        }
    }

    /// Wraps [`World`] into [`WorldShare`].
    #[inline(always)]
    pub fn wrap(world: World) -> Self {
        WorldShare { inner: world }
    }
}

/// A reference to [`World`] that is guaranteed to be not shared with other threads.
/// It can only be created from mutable reference to `World` which is not sendable.
///
/// It bypasses some threading checks and allows access to deferred actions.
///
/// # Examples
///
/// [`WorldLocal`] intentionally doesn't implement `Send` or `Sync`.
///
/// ```compile_fail
/// # use edict::world::WorldLocal;
/// fn test_send<T: core::marker::Send>() {}
///
/// test_send::<WorldLocal>;
/// ```
///
/// ```compile_fail
/// # use edict::world::WorldLocal;
/// fn test_sync<T: core::marker::Sync>() {}
///
/// test_sync::<WorldLocal>;
/// ```
///
/// ```compile_fail
/// # use edict::world::WorldLocal;
/// fn test_send<T: core::marker::Send>() {}
///
/// test_send::<&WorldLocal>;
/// ```
///
/// ```compile_fail
/// # use edict::world::WorldLocal;
/// fn test_send<T: core::marker::Send>() {}
///
/// test_send::<&mut WorldLocal>;
/// ```
#[repr(transparent)]
pub struct WorldLocal {
    inner: World,
}

impl From<WorldLocal> for World {
    #[inline]
    fn from(world: WorldLocal) -> Self {
        world.inner
    }
}

impl From<World> for WorldLocal {
    #[inline]
    fn from(world: World) -> Self {
        WorldLocal { inner: world }
    }
}

impl Deref for WorldLocal {
    type Target = World;

    #[inline(always)]
    fn deref(&self) -> &World {
        &self.inner
    }
}

impl DerefMut for WorldLocal {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut World {
        &mut self.inner
    }
}

impl Debug for WorldLocal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        <World as Debug>::fmt(&self.inner, f)
    }
}

impl WorldLocal {
    #[inline(always)]
    fn wrap_mut(world: &mut World) -> &mut Self {
        // Safety: #[repr(transparent)] allows this cast.
        unsafe { &mut *(world as *mut World as *mut Self) }
    }

    /// Defer execution of the function.
    pub fn defer(&self, f: impl FnOnce(&mut World) + 'static) {
        // Safety:
        // Reference to inner action buffer is never given out, it is used only
        // to record actions from hooks on main thread.
        //
        // This is main thread since this function is called from `WorldLocal`.
        unsafe {
            let action_buffer = &mut *self.inner.action_buffer.get();
            let mut action_encoder = LocalActionEncoder::new(action_buffer, &self.inner.entities);
            action_encoder.closure(f);
        }
    }
}

pub(crate) fn register_bundle<B: ComponentBundleDesc>(
    registry: &mut ComponentRegistry,
    bundle: &B,
) {
    bundle.with_components(|infos| {
        for info in infos {
            registry.get_or_register_raw(info.clone());
        }
    });
}

pub(crate) fn assert_registered_bundle<B: BundleDesc>(
    registry: &mut ComponentRegistry,
    bundle: &B,
) {
    bundle.with_ids(|ids| {
        for (idx, id) in ids.iter().enumerate() {
            if registry.get_info(*id).is_none() {
                panic!(
                    "Component {:?} - ({}[{}]) is not registered",
                    id,
                    type_name::<B>(),
                    idx
                );
            }
        }
    })
}

pub(crate) fn register_one<T: Component>(registry: &mut ComponentRegistry) -> &ComponentInfo {
    registry.get_or_register::<T>()
}

pub(crate) fn assert_registered_one<T: 'static>(
    registry: &mut ComponentRegistry,
) -> &ComponentInfo {
    match registry.get_info(type_id::<T>()) {
        Some(info) => info,
        None => panic!(
            "Component {}({:?}) is not registered",
            type_name::<T>(),
            type_id::<T>()
        ),
    }
}
