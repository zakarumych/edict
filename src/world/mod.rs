//! Self-contained ECS [`World`].

use alloc::{vec, vec::Vec};
use core::{
    any::{type_name, TypeId},
    cell::Cell,
    convert::TryFrom,
    fmt::{self, Debug},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU64, Ordering},
};

use atomicell::{Ref, RefMut};

use crate::{
    action::{ActionBuffer, ActionChannel, ActionEncoder, ActionSender},
    archetype::Archetype,
    bundle::{BundleDesc, ComponentBundleDesc},
    component::{Component, ComponentInfo, ComponentRegistry},
    entity::{AliveEntity, Entity, EntityId, EntityLoc, EntityRef, EntitySet, NoSuchEntity},
    epoch::{EpochCounter, EpochId},
    res::Res,
};

use self::edges::Edges;

pub(crate) use self::spawn::iter_reserve_hint;

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
            if $world.execute_action_buffer {
                ActionBuffer::execute(&mut buffer, $world);
            }
            $world.action_buffer = Some(buffer);
            result
        }
    };
}

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
    execute_action_buffer: bool,

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
    #[inline(always)]
    pub fn archetype_set_id(&self) -> u64 {
        self.archetypes.id()
    }

    /// Looks up entity location and returns entity with location and bound
    /// to the immutable world borrow, ensuring that entity stays alive
    /// and in the same location.
    pub fn lookup(&self, entity: impl Entity) -> Result<EntityLoc<'_>, NoSuchEntity> {
        let entity = entity.entity_loc(&self.entities).ok_or(NoSuchEntity)?;
        Ok(entity)
    }

    /// Returns entity reference
    /// that can be used to access entity's components,
    /// insert or remove components, despawn entity etc.
    pub fn entity(&mut self, entity: impl Entity) -> Result<EntityRef<'_>, NoSuchEntity> {
        let entity = entity.entity_ref(self).ok_or(NoSuchEntity)?;
        Ok(entity)
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
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn has_component<T: 'static>(&self, entity: impl AliveEntity) -> bool {
        let loc = entity.locate(&self.entities);
        if loc.arch == u32::MAX {
            return false;
        }
        self.archetypes[loc.arch as usize].has_component(TypeId::of::<T>())
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
    pub(crate) fn entities(&self) -> &EntitySet {
        &self.entities
    }

    /// Runs world maintenance.
    ///
    /// Users typically do not need to call this method,
    /// it is automatically called in every method that borrows world mutably.
    ///
    /// The only observable effect of manual call to this method
    /// is execution of actions encoded with [`ActionSender`].
    #[inline(always)]
    fn maintenance(&mut self) {
        let epoch = self.epoch.current_mut();
        let archetype = &mut self.archetypes[0];
        self.entities
            .spawn_allocated(|id| archetype.spawn(id, (), epoch));
    }

    pub(crate) unsafe fn with_buffer<R>(
        &mut self,
        buffer: &mut ActionBuffer,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let old_action_buffer = self.action_buffer.replace(core::mem::take(buffer));
        let execute_action_buffer = std::mem::take(&mut self.execute_action_buffer);
        let r = f(self);
        let action_buffer = self
            .action_buffer
            .replace(old_action_buffer.unwrap_unchecked());
        self.execute_action_buffer = execute_action_buffer;
        *buffer = action_buffer.unwrap_unchecked();
        r
    }

    /// Executes closure with [`ActionEncoder`] instance bound to this [`World`].
    pub fn with_encoder<R>(&mut self, f: impl FnOnce(&Self, ActionEncoder<'_>) -> R) -> R {
        with_buffer!(self, buffer => {
            let encoder = ActionEncoder::new(buffer, &self.entities);
            f(self, encoder)
        })
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
