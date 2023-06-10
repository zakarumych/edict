//! Self-contained ECS [`World`].

use alloc::{borrow::ToOwned, vec, vec::Vec};
use core::{
    any::{type_name, TypeId},
    cell::Cell,
    convert::TryFrom,
    fmt::{self, Debug},
    hash::Hash,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU64, Ordering},
};

use atomicell::{Ref, RefMut};

use crate::{
    action::{ActionBuffer, ActionChannel, ActionEncoder, ActionSender},
    archetype::{chunk_idx, Archetype},
    bundle::{
        Bundle, BundleDesc, ComponentBundle, ComponentBundleDesc, DynamicBundle,
        DynamicComponentBundle,
    },
    component::{Component, ComponentInfo, ComponentRegistry},
    entity::{AliveEntity, Entity, EntityId, EntityLoc, EntitySet, Location, NoSuchEntity},
    epoch::{EpochCounter, EpochId},
    query::{DefaultQuery, Fetch, IntoQuery, Query, QueryItem},
    res::Res,
    view::View,
};

use self::edges::Edges;

pub use self::builder::WorldBuilder;

/// Takes internal action buffer and
/// runs expression with it.
/// After expression completes, action buffer is executed
/// and then returned to the world.
///
/// While action buffer is taken,
/// all paths would either use world's methods with explicit buffer
/// or use `with_buffer` method,
/// ensuring that action buffer is always present
/// when this macro is called.
macro_rules! with_buffer {
    ($world:ident, $buffer:ident => $expr:expr) => {
        unsafe {
            let mut buffer = $world.action_buffer.take().unwrap_unchecked();
            let result = {
                let $buffer = &mut buffer;
                $expr
            };
            ActionBuffer::execute(&mut buffer, $world);
            $world.action_buffer = Some(buffer);
            result
        }
    };
}

mod builder;
mod edges;
mod get;
mod insert;
mod remove;
mod spawn;
mod view;

/// Limits on reserving of space for entities and components
/// in archetypes when `spawn_batch` is used.
const MAX_SPAWN_RESERVE: usize = 1024;

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

    res: Res,

    /// Internal action encoder.
    /// This encoder is used to record commands from component hooks.
    /// Commands are immediately executed at the end of the mutating call.
    action_buffer: Option<ActionBuffer>,

    action_channel: ActionChannel,
}

unsafe impl Sync for World {}

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
    #[inline]
    pub fn archetype_set_id(&self) -> u64 {
        self.archetypes.id()
    }

    pub fn alive(&self, entity: impl Entity) -> Result<EntityLoc, NoSuchEntity> {
        let loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;
        Ok(EntityLoc::new(entity.id(), loc))
    }

    /// Returns current world epoch.
    ///
    /// This value can be modified concurrently if [`&World`] is shared.
    /// As it increases monotonically, returned value can be safely assumed as a lower bound.
    ///
    /// [`&World`]: World
    #[inline]
    pub fn epoch(&self) -> EpochId {
        self.epoch.current()
    }

    /// Returns atomic reference to epoch counter.
    #[inline]
    pub fn epoch_counter(&self) -> &EpochCounter {
        &self.epoch
    }

    /// Checks if entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn has_component<T: 'static>(&self, id: EntityId) -> Result<bool, NoSuchEntity> {
        let (archetype_idx, _idx) = self.entities.get_location(id).ok_or(NoSuchEntity)?;
        if archetype_idx == u32::MAX {
            return Ok(false);
        }
        Ok(self.archetypes[archetype_idx as usize].has_component(TypeId::of::<T>()))
    }

    /// Checks if entity is alive.
    #[inline]
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

    /// Inserts resource instance.
    /// Old value is replaced.
    ///
    /// To access resource, use [`World::get_resource`] and [`World::get_resource_mut`] methods.
    ///
    /// [`World::get_resource`]: struct.World.html#method.get_resource
    /// [`World::get_resource_mut`]: struct.World.html#method.get_resource_mut
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// *world.get_resource_mut::<i32>().unwrap() = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn insert_resource<T: 'static>(&mut self, resource: T) {
        self.res.insert(resource)
    }

    /// Returns reference to the resource instance.
    /// Inserts new instance if it does not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let value = world.with_resource(|| 42i32);
    /// assert_eq!(*value, 42);
    /// *value = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn with_resource<T: 'static>(&mut self, f: impl FnOnce() -> T) -> &mut T {
        self.res.with(f)
    }

    /// Returns reference to the resource instance.
    /// Inserts new instance if it does not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let value = world.with_default_resource::<u32>();
    /// assert_eq!(*value, 0);
    /// *value = 11;
    /// assert_eq!(*world.get_resource::<u32>().unwrap(), 11);
    /// ```
    pub fn with_default_resource<T: Default + 'static>(&mut self) -> &mut T {
        self.res.with(T::default)
    }

    /// Remove resource instance.
    /// Returns `None` if resource was not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// world.remove_resource::<i32>();
    /// assert!(world.get_resource::<i32>().is_none());
    /// ```
    pub fn remove_resource<T: 'static>(&mut self) -> Option<T> {
        self.res.remove()
    }

    /// Returns some reference to potentially `!Sync` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// # use core::cell::Cell;
    /// let mut world = World::new();
    /// world.insert_resource(Cell::new(42i32));
    /// unsafe {
    ///     assert_eq!(42, world.get_local_resource::<Cell<i32>>().unwrap().get());
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// User must ensure that obtained immutable reference is safe.
    /// For example calling this method from "main" thread is always safe.
    ///
    /// If `T` is `Sync` then this method is also safe.
    /// In this case prefer to use [`World::get_resource`] method instead.
    ///
    /// If user has mutable access to [`World`] this function is guaranteed to be safe to call.
    /// [`WorldLocal`] wrapper can be used to avoid `unsafe` blocks.
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// let local = world.local();
    /// assert_eq!(42, *local.get_resource::<i32>().unwrap());
    /// ```
    pub unsafe fn get_local_resource<T: 'static>(&self) -> Option<Ref<T>> {
        unsafe { self.res.get_local() }
    }

    /// Returns some mutable reference to potentially `!Send` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// # use core::cell::Cell;
    /// let mut world = World::new();
    /// world.insert_resource(Cell::new(42i32));
    /// unsafe {
    ///     *world.get_local_resource_mut::<Cell<i32>>().unwrap().get_mut() = 11;
    ///     assert_eq!(11, world.get_local_resource::<Cell<i32>>().unwrap().get());
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// User must ensure that obtained mutable reference is safe.
    /// For example calling this method from "main" thread is always safe.
    ///
    /// If `T` is `Send` then this method is also safe.
    /// In this case prefer to use [`World::get_resource_mut`] method instead.
    ///
    /// If user has mutable access to [`World`] this function is guaranteed to be safe to call.
    /// [`WorldLocal`] wrapper can be used to avoid `unsafe` blocks.
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// let local = world.local();
    /// *local.get_resource_mut::<i32>().unwrap() = 11;
    /// ```
    pub unsafe fn get_local_resource_mut<T: 'static>(&self) -> Option<RefMut<T>> {
        unsafe { self.res.get_local_mut() }
    }

    /// Returns some reference to `Sync` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// assert!(world.get_resource::<i32>().is_none());
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// ```
    pub fn get_resource<T: Sync + 'static>(&self) -> Option<Ref<T>> {
        self.res.get()
    }

    /// Returns reference to `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.expect_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.expect_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn expect_resource<T: Sync + 'static>(&self) -> Ref<T> {
        self.res.get().unwrap()
    }

    /// Returns a copy for the `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.copy_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(world.copy_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn copy_resource<T: Copy + Sync + 'static>(&self) -> T {
        *self.res.get().unwrap()
    }

    /// Returns some mutable reference to `Send` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// assert!(world.get_resource_mut::<i32>().is_none());
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// *world.get_resource_mut::<i32>().unwrap() = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn get_resource_mut<T: Send + 'static>(&self) -> Option<RefMut<T>> {
        self.res.get_mut()
    }

    /// Returns mutable reference to `Send` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.expect_resource_mut::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// *world.expect_resource_mut::<i32>() = 11;
    /// assert_eq!(*world.expect_resource_mut::<i32>(), 11);
    /// ```
    #[track_caller]
    pub fn expect_resource_mut<T: Send + 'static>(&self) -> RefMut<T> {
        self.res.get_mut().unwrap()
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
    pub fn local(&mut self) -> WorldLocal<'_> {
        WorldLocal {
            world: self,
            marker: PhantomData,
        }
    }

    /// Reset all possible leaks on resources.
    /// Mutable reference guarantees that no borrows are active.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, atomicell::RefMut};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    ///
    /// // Leaking reference to resource causes it to stay borrowed.
    /// let value: &mut i32 = RefMut::leak(world.get_resource_mut().unwrap());
    /// *value = 11;
    ///
    /// // Reset all borrows including leaked ones.
    /// world.undo_resource_leak();
    ///
    /// // Borrow succeeds.
    /// assert_eq!(world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn undo_resource_leak(&mut self) {
        self.res.undo_leak()
    }

    /// Returns iterator over resource types.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::{any::TypeId, collections::HashSet};
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// world.insert_resource(1.5f32);
    /// assert_eq!(
    ///     world.resource_types().collect::<HashSet<_>>(),
    ///     HashSet::from([TypeId::of::<i32>(), TypeId::of::<f32>()]),
    /// );
    /// ```
    pub fn resource_types(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.res.resource_types()
    }

    /// Returns [`ActionSender`] instance bound to this [`World`].\
    /// [`ActionSender`] can be used to send actions to the [`World`] from
    /// other threads and async tasks.
    ///
    /// [`ActionSender`] API is similar to [`ActionEncoder`]
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
    /// let action_sender = world.action_sender();
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
    pub fn action_sender(&self) -> ActionSender {
        self.action_channel.sender()
    }

    /// Executes actions received from [`ActionSender`] instances
    /// bound to this [`World`].
    ///
    /// See [`World::action_sender`] for more information.
    pub fn execute_received_actions(&mut self) {
        self.maintenance();
        with_buffer!(self, buffer => {
            self.action_channel.fetch();
            while let Some(f) = self.action_channel.execute() {
                f(self, buffer);
            }
        })
    }

    /// Returns [`EntitySet`] from the [`World`].
    pub(crate) fn entity_set(&self) -> &EntitySet {
        &self.entities
    }

    /// Temporary replaces internal action buffer with provided one.
    #[inline]
    pub(crate) fn with_buffer(&mut self, buffer: &mut ActionBuffer, f: impl FnOnce(&mut World)) {
        let action_buffer = self.action_buffer.take();
        self.action_buffer = Some(core::mem::take(buffer));
        f(self);
        *buffer = self.action_buffer.take().unwrap();
        self.action_buffer = action_buffer;
    }

    /// Runs world maintenance.
    ///
    /// Users typically do not need to call this method,
    /// it is automatically called in every method that borrows world mutably.
    ///
    /// The only observable effect of manual call to this method
    /// is execution of actions encoded with [`ActionSender`].
    #[inline]
    fn maintenance(&mut self) {
        let epoch = self.epoch.current_mut();
        let archetype = &mut self.archetypes[0];
        self.entities
            .spawn_allocated(|id| archetype.spawn(id, (), epoch));
    }
}

/// Error returned in case specified entity does not contain
/// component of required type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MissingComponents;

impl fmt::Display for MissingComponents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Specified component is not found in entity")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MissingComponents {}

/// Error returned if either entity reference is invalid
/// or component of required type is not found for an entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityError {
    /// Error returned in case specified [`EntityId`]
    /// does not reference any live entity in the [`World`].
    NoSuchEntity,

    /// Error returned in case specified entity does not contain
    /// component of required type.
    MissingComponents,
}

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchEntity => fmt::Display::fmt(&NoSuchEntity, f),
            Self::MissingComponents => fmt::Display::fmt(&MissingComponents, f),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EntityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoSuchEntity => Some(&NoSuchEntity),
            Self::MissingComponents => Some(&MissingComponents),
        }
    }
}

impl From<NoSuchEntity> for EntityError {
    fn from(_: NoSuchEntity) -> Self {
        EntityError::NoSuchEntity
    }
}

impl From<MissingComponents> for EntityError {
    fn from(_: MissingComponents) -> Self {
        EntityError::MissingComponents
    }
}

impl PartialEq<NoSuchEntity> for EntityError {
    fn eq(&self, _: &NoSuchEntity) -> bool {
        matches!(self, EntityError::NoSuchEntity)
    }
}

impl PartialEq<MissingComponents> for EntityError {
    fn eq(&self, _: &MissingComponents) -> bool {
        matches!(self, EntityError::MissingComponents)
    }
}

/// Error returned by [`World::query_one`] method family
/// when query is not satisfied by the entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryOneError {
    /// Error returned in case specified [`EntityId`]
    /// does not reference any live entity in the [`World`].
    NoSuchEntity,

    /// Error returned in case specified entity does not contain
    /// component of required type.
    NotSatisfied,
}

impl fmt::Display for QueryOneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchEntity => fmt::Display::fmt(&NoSuchEntity, f),
            Self::NotSatisfied => f.write_str("Query is not satisfied"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for QueryOneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoSuchEntity => Some(&NoSuchEntity),
            Self::NotSatisfied => None,
        }
    }
}

impl From<NoSuchEntity> for QueryOneError {
    fn from(_: NoSuchEntity) -> Self {
        QueryOneError::NoSuchEntity
    }
}

impl PartialEq<NoSuchEntity> for QueryOneError {
    fn eq(&self, _: &NoSuchEntity) -> bool {
        matches!(self, QueryOneError::NoSuchEntity)
    }
}

/// Inserts component.
/// This function uses different code to assign component when it already exists on entity.
fn insert_component<T, C>(
    world: &mut World,
    id: EntityId,
    value: T,
    into_component: impl FnOnce(T) -> C,
    set_component: impl FnOnce(&mut C, T, ActionEncoder),
    buffer: &mut ActionBuffer,
) where
    C: Component,
{
    let Location {
        archetype: src_archetype,
        idx,
    } = world.entities.get_location(id).unwrap();
    debug_assert!(src_archetype < u32::MAX, "Allocated entities were spawned");

    if world.archetypes[src_archetype as usize].has_component(TypeId::of::<C>()) {
        let component = unsafe {
            world.archetypes[src_archetype as usize].get_mut::<C>(idx, world.epoch.current_mut())
        };

        set_component(
            component,
            value,
            ActionEncoder::new(buffer, &world.entities),
        );

        return;
    }

    let component = into_component(value);

    let dst_archetype = world.edges.insert(
        TypeId::of::<C>(),
        &mut world.registry,
        &mut world.archetypes,
        src_archetype,
        |registry| registry.get_or_register::<C>(),
    );

    debug_assert_ne!(src_archetype, dst_archetype);

    let (before, after) = world
        .archetypes
        .split_at_mut(src_archetype.max(dst_archetype) as usize);

    let (src, dst) = match src_archetype < dst_archetype {
        true => (&mut before[src_archetype as usize], &mut after[0]),
        false => (&mut after[0], &mut before[dst_archetype as usize]),
    };

    let (dst_idx, opt_src_id) =
        unsafe { src.insert(id, dst, idx, component, world.epoch.current_mut()) };

    world
        .entities
        .set_location(id, Location::new(dst_archetype, dst_idx));

    if let Some(src_id) = opt_src_id {
        world
            .entities
            .set_location(src_id, Location::new(src_archetype, idx));
    }
}

/// A reference to [`World`] that allows to fetch local resources.
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
pub struct WorldLocal<'a> {
    world: &'a mut World,
    marker: PhantomData<Cell<World>>,
}

impl Deref for WorldLocal<'_> {
    type Target = World;

    fn deref(&self) -> &Self::Target {
        self.world
    }
}

impl DerefMut for WorldLocal<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.world
    }
}

impl Debug for WorldLocal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        <World as Debug>::fmt(&*self.world, f)
    }
}

impl WorldLocal<'_> {
    /// Returns some reference to a resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::rc::Rc;
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let world = world.local();
    /// assert!(world.get_resource::<Rc<i32>>().is_none());
    /// ```
    ///
    /// ```
    /// # use std::rc::Rc;
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let mut world = world.local();
    /// world.insert_resource(Rc::new(42i32));
    /// assert_eq!(**world.get_resource::<Rc<i32>>().unwrap(), 42);
    /// ```
    pub fn get_resource<T: 'static>(&self) -> Option<Ref<T>> {
        // Safety:
        // Mutable reference to `Res` ensures this is the "main" thread.
        unsafe { self.world.res.get_local() }
    }

    /// Returns reference to a resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let world = world.local();
    /// world.expect_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let mut world = world.local();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.expect_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn expect_resource<T: 'static>(&self) -> Ref<T> {
        // Safety:
        // Mutable reference to `Res` ensures this is the "main" thread.
        unsafe { self.world.res.get_local() }.unwrap()
    }

    /// Returns a copy for the a resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let world = world.local();
    /// world.copy_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let mut world = world.local();
    /// world.insert_resource(42i32);
    /// assert_eq!(world.copy_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn copy_resource<T: Copy + 'static>(&self) -> T {
        // Safety:
        // Mutable reference to `Res` ensures this is the "main" thread.
        *unsafe { self.world.res.get_local() }.unwrap()
    }

    /// Returns some mutable reference to a resource.
    /// Returns none if resource is not found.
    pub fn get_resource_mut<T: 'static>(&self) -> Option<RefMut<T>> {
        // Safety:
        // Mutable reference to `Res` ensures this is the "main" thread.
        unsafe { self.world.res.get_local_mut() }
    }

    /// Returns mutable reference to a resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let mut world = world.local();
    /// let world = world.local();
    /// world.expect_resource_mut::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let mut world = world.local();
    /// world.insert_resource(42i32);
    /// *world.expect_resource_mut::<i32>() = 11;
    /// assert_eq!(*world.expect_resource_mut::<i32>(), 11);
    /// ```
    #[track_caller]
    pub fn expect_resource_mut<T: 'static>(&self) -> RefMut<T> {
        // Safety:
        // Mutable reference to `Res` ensures this is the "main" thread.
        unsafe { self.world.res.get_local_mut() }.unwrap()
    }
}

fn register_bundle<B: ComponentBundleDesc>(registry: &mut ComponentRegistry, bundle: &B) {
    bundle.with_components(|infos| {
        for info in infos {
            registry.get_or_register_raw(info.clone());
        }
    });
}

fn assert_registered_bundle<B: BundleDesc>(registry: &mut ComponentRegistry, bundle: &B) {
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

fn register_one<T: Component>(registry: &mut ComponentRegistry) -> &ComponentInfo {
    registry.get_or_register::<T>()
}

fn assert_registered_one<T: 'static>(registry: &mut ComponentRegistry) -> &ComponentInfo {
    match registry.get_info(TypeId::of::<T>()) {
        Some(info) => info,
        None => panic!(
            "Component {}({:?}) is not registered",
            type_name::<T>(),
            TypeId::of::<T>()
        ),
    }
}
