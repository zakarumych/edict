//! Self-contained ECS [`World`].

use core::{
    any::{type_name, TypeId},
    cell::Cell,
    convert::TryFrom,
    fmt::{self, Debug},
    hash::Hash,
    iter::FromIterator,
    iter::FusedIterator,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU64, Ordering},
};

use alloc::vec::Vec;
use atomicell::{Ref, RefMut};

use crate::{
    action::{ActionBuffer, ActionEncoder},
    archetype::{chunk_idx, Archetype},
    bundle::{
        Bundle, BundleDesc, ComponentBundle, ComponentBundleDesc, DynamicBundle,
        DynamicComponentBundle,
    },
    component::{Component, ComponentInfo, ComponentRegistry},
    entity::{EntityId, EntitySet},
    epoch::{EpochCounter, EpochId},
    query::{Fetch, IntoQuery, Query, QueryItem},
    relation::{OriginComponent, Relation, TargetComponent},
    res::Res,
};

use self::edges::Edges;

pub use self::{
    builder::WorldBuilder,
    query::{QueryOne, QueryRef},
};

mod builder;
mod edges;
mod query;

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
            Err(_) => panic!("Too many archetypes"),
            Ok(len) => len,
        };

        let new_archetype = f(&self.archetypes);
        self.archetypes.push(new_archetype);
        self.id = NEXT_ARCHETYPE_SET_ID.fetch_add(1, Ordering::Relaxed);
        len
    }
}

pub(crate) fn iter_reserve_hint(iter: &impl Iterator) -> usize {
    let (lower, upper) = iter.size_hint();
    match (lower, upper) {
        (lower, None) => lower,
        (lower, Some(upper)) => {
            // Iterator is consumed in full, so reserve at least `lower`.
            lower.max(upper.min(MAX_SPAWN_RESERVE))
        }
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
    cached_action_buffer: Option<ActionBuffer>,
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

macro_rules! with_buffer {
    ($world:ident, $buffer:ident => $expr:expr) => {{
        let mut buffer = $world.cached_action_buffer.take().unwrap();
        let result = {
            let $buffer = &mut buffer;
            $expr
        };
        ActionBuffer::execute(&mut buffer, $world);
        $world.cached_action_buffer = Some(buffer);
        result
    }};
}

impl World {
    /// Returns new instance of [`World`].
    /// Created [`World`] instance contains no entities.
    #[inline]
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Returns new instance of [`WorldBuilder`].
    /// This allows pre-register component types and override their behavior.
    #[inline]
    pub const fn builder() -> WorldBuilder {
        WorldBuilder::new()
    }

    /// Explicitly registers component type.
    ///
    /// Unlike [`WorldBuilder::register_component`] method, this method does not return reference to component configuration,
    /// once [`World`] is created overriding component behavior is not possible.
    ///
    /// Component types are implicitly registered on first use by most methods.
    /// This method is only needed if you want to use component type using
    /// [`World::insert_external`], [`World::insert_external_bundle`] or [`World::spawn_external`].
    pub fn ensure_component_registered<T>(&mut self)
    where
        T: Component,
    {
        self.registry.ensure_component_registered::<T>();
    }

    /// Explicitly registers bundle of component types.
    ///
    /// This method is only needed if you want to use bundle of component types using
    /// [`World::insert_external_bundle`] or [`World::spawn_external`].
    pub fn ensure_bundle_registered<B>(&mut self)
    where
        B: ComponentBundle,
    {
        register_bundle(&mut self.registry, &PhantomData::<B>);
    }

    /// Explicitly registers external type.
    ///
    /// Unlike [`WorldBuilder::register_external`] method, this method does not return reference to component configuration,
    /// once [`World`] is created overriding component behavior is not possible.
    ///
    /// External component types are not implicitly registered on first use.
    /// This method is needed if you want to use component type with
    /// [`World::insert_external`], [`World::insert_external_bundle`] or [`World::spawn_external`].
    pub fn ensure_external_registered<T>(&mut self)
    where
        T: 'static,
    {
        self.registry.ensure_external_registered::<T>();
    }

    /// Returns unique identified of archetype set.
    /// This ID changes each time new archetype is added or removed.
    /// IDs of different worlds are never equal within the same process.
    #[inline]
    pub fn archetype_set_id(&self) -> u64 {
        self.archetypes.id()
    }

    /// Reserves new entity id.
    ///
    /// The entity will be materialized before first mutation on the world happens.
    /// Until then entity is alive and belongs to empty archetype.
    /// Entity will be alive until [`World::despawn`] is called with returned [`EntityId`] handle.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let entity = world.allocate();
    /// assert_eq!(world.is_alive(entity), true);
    /// world.despawn(entity).unwrap();
    /// assert_eq!(world.is_alive(entity), false);
    /// ```
    #[inline]
    pub fn allocate(&self) -> EntityId {
        self.entities.alloc()
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// Returns [`EntityId`] handle to the newly spawned entity.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`World::despawn`] is called with returned [`EntityId`] handle.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn((ExampleComponent,));
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(true));
    /// let ExampleComponent = world.remove(entity).unwrap();
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(false));
    /// ```
    #[inline]
    pub fn spawn<B>(&mut self, bundle: B) -> EntityId
    where
        B: DynamicComponentBundle,
    {
        self.spawn_impl(bundle, register_bundle::<B>)
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// Returns [`EntityId`] handle to the newly spawned entity.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`World::despawn`] is called with returned [`EntityId`] handle.
    ///
    /// All components from the bundle must be previously registered.
    /// If component in bundle implements [`Component`] it could be registered implicitly
    /// on first by [`World::spawn`], [`World::spawn_batch`], [`World::insert`] or [`World::insert_bundle`].
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`] or later by [`World::ensure_component_registered`].
    /// Non [`Component`] types must be pre-registered by [`WorldBuilder::register_external`] or later by [`World::ensure_external_registered`].
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    /// world.ensure_component_registered::<ExampleComponent>();
    /// let entity = world.spawn_external((42u32, ExampleComponent));
    /// assert_eq!(world.has_component::<u32>(entity), Ok(true));
    /// assert_eq!(world.remove(entity), Ok(42u32));
    /// assert_eq!(world.has_component::<u32>(entity), Ok(false));
    /// ```
    #[inline]
    pub fn spawn_external<B>(&mut self, bundle: B) -> EntityId
    where
        B: DynamicBundle,
    {
        self.spawn_impl(bundle, assert_registered_bundle::<B>)
    }

    fn spawn_impl<B, F>(&mut self, bundle: B, register_bundle: F) -> EntityId
    where
        B: DynamicBundle,
        F: FnOnce(&mut ComponentRegistry, &B),
    {
        if !bundle.valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        self.spawn_allocated();

        let entity = self.entities.spawn();

        let archetype_idx = self.edges.spawn(
            &mut self.registry,
            &mut self.archetypes,
            &bundle,
            |registry| register_bundle(registry, &bundle),
        );
        let epoch = self.epoch.next_mut();
        let idx = self.archetypes[archetype_idx as usize].spawn(entity, bundle, epoch);
        self.entities.set_location(entity.id(), archetype_idx, idx);
        entity
    }

    /// Returns an iterator which spawns and yield entities
    /// using bundles yielded from provided bundles iterator.
    ///
    /// When bundles iterator returns `None`, returned iterator returns `None` too.
    ///
    /// If bundles iterator is fused, returned iterator is fused too.
    /// If bundles iterator is double-ended, returned iterator is double-ended too.
    /// If bundles iterator has exact size, returned iterator has exact size too.
    ///
    /// Skipping items on returned iterator will cause bundles iterator skip bundles and not spawn entities.
    ///
    /// Returned iterator attempts to optimize storage allocation for entities
    /// if consumed with functions like `fold`, `rfold`, `for_each` or `collect`.
    ///
    /// When returned iterator is dropped, no more entities will be spawned
    /// even if bundles iterator has items left.
    #[inline]
    pub fn spawn_batch<B, I>(&mut self, bundles: I) -> SpawnBatch<'_, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: ComponentBundle,
    {
        self.spawn_batch_impl(bundles, |registry| {
            register_bundle(registry, &PhantomData::<B>)
        })
    }

    /// Returns an iterator which spawns and yield entities
    /// using bundles yielded from provided bundles iterator.
    ///
    /// When bundles iterator returns `None`, returned iterator returns `None` too.
    ///
    /// If bundles iterator is fused, returned iterator is fused too.
    /// If bundles iterator is double-ended, returned iterator is double-ended too.
    /// If bundles iterator has exact size, returned iterator has exact size too.
    ///
    /// Skipping items on returned iterator will cause bundles iterator skip bundles and not spawn entities.
    ///
    /// Returned iterator attempts to optimize storage allocation for entities
    /// if consumed with functions like `fold`, `rfold`, `for_each` or `collect`.
    ///
    /// When returned iterator is dropped, no more entities will be spawned
    /// even if bundles iterator has items left.
    ///
    /// All components from the bundle must be previously registered.
    /// If component in bundle implements [`Component`] it could be registered implicitly
    /// on first by [`World::spawn`], [`World::spawn_batch`], [`World::insert`] or [`World::insert_bundle`].
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`] or later by [`World::ensure_component_registered`].
    /// Non [`Component`] types must be pre-registered by [`WorldBuilder::register_external`] or later by [`World::ensure_external_registered`].
    #[inline]
    pub fn spawn_batch_external<B, I>(&mut self, bundles: I) -> SpawnBatch<'_, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: Bundle,
    {
        self.spawn_batch_impl(bundles, |registry| {
            assert_registered_bundle(registry, &PhantomData::<B>)
        })
    }

    fn spawn_batch_impl<B, I, F>(
        &mut self,
        bundles: I,
        register_bundle: F,
    ) -> SpawnBatch<'_, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: Bundle,
        F: FnOnce(&mut ComponentRegistry),
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        self.spawn_allocated();

        let archetype_idx = self.edges.insert_bundle(
            &mut self.registry,
            &mut self.archetypes,
            0,
            &PhantomData::<I::Item>,
            register_bundle,
        );

        let epoch = self.epoch.next_mut();

        let archetype = &mut self.archetypes[archetype_idx as usize];
        let entities = &mut self.entities;

        SpawnBatch {
            bundles: bundles.into_iter(),
            epoch,
            archetype_idx,
            archetype,
            entities,
        }
    }

    pub(crate) fn spawn_reserve<B>(&mut self, additional: usize)
    where
        B: Bundle,
    {
        self.entities.reserve(additional);

        let archetype_idx = self.edges.insert_bundle(
            &mut self.registry,
            &mut self.archetypes,
            0,
            &PhantomData::<B>,
            |registry| assert_registered_bundle(registry, &PhantomData::<B>),
        );

        let archetype = &mut self.archetypes[archetype_idx as usize];
        archetype.reserve(additional);
    }

    /// Despawns an entity with specified id.
    /// Returns [`Err(NoSuchEntity)`] if entity does not exists.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn((ExampleComponent,));
    /// assert!(world.despawn(entity).is_ok(), "Entity should be despawned by this call");
    /// assert!(world.despawn(entity).is_err(), "Already despawned");
    /// ```
    #[inline]
    pub fn despawn(&mut self, entity: EntityId) -> Result<(), NoSuchEntity> {
        with_buffer!(self, buffer => self.despawn_with_buffer(entity, buffer))
    }

    #[inline]
    pub(crate) fn despawn_with_buffer(
        &mut self,
        entity: EntityId,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity> {
        self.spawn_allocated();

        let (archetype, idx) = self.entities.despawn(entity)?;

        let encoder = ActionEncoder::new(buffer, &self.entities);
        let opt_id =
            unsafe { self.archetypes[archetype as usize].despawn_unchecked(entity, idx, encoder) };

        if let Some(id) = opt_id {
            self.entities.set_location(id, archetype, idx)
        }

        Ok(())
    }

    /// Searches for an entity with specified index.
    /// Returns `Ok(entity)` if entity with specified index exists.
    /// Returns `Err(NoSuchEntity)` otherwise.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::{World, NoSuchEntity}, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn((ExampleComponent,));
    /// assert_eq!(world.find_entity(entity.id()), Ok(entity));
    /// assert_eq!(world.find_entity(u32::MAX), Err(NoSuchEntity), "It would require to create u32::MAX entities to make this Ok(_)");
    /// ```
    #[inline]
    pub fn find_entity(&self, idx: u32) -> Result<EntityId, NoSuchEntity> {
        self.entities.find_entity(idx).ok_or(NoSuchEntity)
    }

    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(());
    ///
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert(entity, ExampleComponent).unwrap();
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert<T>(&mut self, entity: EntityId, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        with_buffer!(self, buffer => {
            self.insert_with_buffer(entity, component, buffer)
        })
    }

    #[inline]
    pub(crate) fn insert_with_buffer<T>(
        &mut self,
        entity: EntityId,
        component: T,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        self.insert_impl(entity, component, register_one::<T>, buffer)
    }

    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let entity = world.spawn(());
    ///
    /// assert_eq!(world.has_component::<u32>(entity), Ok(false));
    /// world.ensure_external_registered::<u32>();
    /// world.insert_external(entity, 42u32).unwrap();
    /// assert_eq!(world.has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert_external<T>(&mut self, entity: EntityId, component: T) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        with_buffer!(self, buffer => {
            self.insert_external_with_buffer(entity, component, buffer)
        })
    }

    #[inline]
    pub(crate) fn insert_external_with_buffer<T>(
        &mut self,
        entity: EntityId,
        component: T,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        self.insert_impl(entity, component, assert_registered_one::<T>, buffer)
    }

    pub(crate) fn insert_impl<T, F>(
        &mut self,
        entity: EntityId,
        component: T,
        get_or_register: F,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        T: 'static,
        F: FnOnce(&mut ComponentRegistry) -> &ComponentInfo,
    {
        self.spawn_allocated();

        let (src_archetype, idx) = self.entities.get_location(entity).ok_or(NoSuchEntity)?;

        let epoch = self.epoch.next_mut();

        let encoder = ActionEncoder::new(buffer, &self.entities);

        if self.archetypes[src_archetype as usize].has_component(TypeId::of::<T>()) {
            unsafe {
                self.archetypes[src_archetype as usize].set(entity, idx, component, epoch, encoder);
            }

            return Ok(());
        }

        let dst_archetype = self.edges.insert(
            TypeId::of::<T>(),
            &mut self.registry,
            &mut self.archetypes,
            src_archetype,
            get_or_register,
        );

        debug_assert_ne!(src_archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.insert(entity, dst, idx, component, epoch) };

        self.entities
            .set_location(entity.id(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(())
    }

    /// Removes component from the specified entity and returns its value.
    ///
    /// If entity does not have component of this type, fails with `Err(EntityError::MissingComponent)`.
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn remove<T>(&mut self, entity: EntityId) -> Result<T, EntityError>
    where
        T: 'static,
    {
        self.spawn_allocated();

        let (src_archetype, idx) = self
            .entities
            .get_location(entity)
            .ok_or(EntityError::NoSuchEntity)?;

        if !self.archetypes[src_archetype as usize].has_component(TypeId::of::<T>()) {
            return Err(EntityError::MissingComponents);
        }

        let dst_archetype =
            self.edges
                .remove(&mut self.archetypes, src_archetype, TypeId::of::<T>());

        debug_assert_ne!(src_archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id, component) = unsafe { src.remove(entity, dst, idx) };

        self.entities
            .set_location(entity.id(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(component)
    }

    /// Drops component from the specified entity.
    ///
    /// If entity does not have component of this type, fails with `Err(EntityError::MissingComponent)`.
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn drop<T>(&mut self, entity: EntityId) -> Result<(), EntityError>
    where
        T: 'static,
    {
        self.drop_erased(entity, TypeId::of::<T>())
    }

    /// Drops component from the specified entity.
    ///
    /// If entity does not have component of this type, fails with `Err(EntityError::MissingComponent)`.
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn drop_erased(&mut self, entity: EntityId, id: TypeId) -> Result<(), EntityError> {
        with_buffer!(self, buffer => {
            self.drop_erased_with_buffer(entity, id, buffer)
        })
    }

    #[inline]
    pub(crate) fn drop_erased_with_buffer(
        &mut self,
        entity: EntityId,
        id: TypeId,
        buffer: &mut ActionBuffer,
    ) -> Result<(), EntityError> {
        self.spawn_allocated();

        let (src_archetype, idx) = self
            .entities
            .get_location(entity)
            .ok_or(EntityError::NoSuchEntity)?;

        if !self.archetypes[src_archetype as usize].has_component(id) {
            return Err(EntityError::MissingComponents);
        }

        let dst_archetype = self.edges.remove(&mut self.archetypes, src_archetype, id);

        debug_assert_ne!(src_archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe {
            src.drop_bundle(entity, dst, idx, ActionEncoder::new(buffer, &self.entities))
        };

        self.entities
            .set_location(entity.id(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(())
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(());
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert_bundle(entity, (ExampleComponent,));
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert_bundle<B>(&mut self, entity: EntityId, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        with_buffer!(self, buffer => {
            self.insert_bundle_with_buffer(entity, bundle, buffer)
        })
    }

    #[inline]
    pub(crate) fn insert_bundle_with_buffer<B>(
        &mut self,
        entity: EntityId,
        bundle: B,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        self.insert_bundle_impl(entity, bundle, register_bundle::<B>, buffer)
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(());
    ///
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(false));
    /// assert_eq!(world.has_component::<u32>(entity), Ok(false));
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// world.insert_external_bundle(entity, (ExampleComponent, 42u32));
    ///
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(true));
    /// assert_eq!(world.has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert_external_bundle<B>(
        &mut self,
        entity: EntityId,
        bundle: B,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        with_buffer!(self, buffer => {
            self.insert_external_bundle_with_buffer(entity, bundle, buffer)
        })
    }

    #[inline]
    pub(crate) fn insert_external_bundle_with_buffer<B>(
        &mut self,
        entity: EntityId,
        bundle: B,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        self.insert_bundle_impl(entity, bundle, assert_registered_bundle::<B>, buffer)
    }

    fn insert_bundle_impl<B, F>(
        &mut self,
        entity: EntityId,
        bundle: B,
        register_bundle: F,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
        F: FnOnce(&mut ComponentRegistry, &B),
    {
        if !bundle.valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        self.spawn_allocated();

        let (src_archetype, idx) = self.entities.get_location(entity).ok_or(NoSuchEntity)?;

        if bundle.with_ids(|ids| ids.is_empty()) {
            return Ok(());
        }

        let epoch = self.epoch.next_mut();

        let dst_archetype = self.edges.insert_bundle(
            &mut self.registry,
            &mut self.archetypes,
            src_archetype,
            &bundle,
            |registry| register_bundle(registry, &bundle),
        );

        if dst_archetype == src_archetype {
            unsafe {
                self.archetypes[src_archetype as usize].set_bundle(
                    entity,
                    idx,
                    bundle,
                    epoch,
                    ActionEncoder::new(buffer, &self.entities),
                )
            }
            return Ok(());
        }

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe {
            src.insert_bundle(
                entity,
                dst,
                idx,
                bundle,
                epoch,
                ActionEncoder::new(buffer, &self.entities),
            )
        };

        self.entities
            .set_location(entity.id(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(())
    }

    /// Drops components of the specified entity with type from the bundle.
    /// Skips any component type entity doesn't have.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This method works for bundles of both [`Component`] implementations and external component types alike.
    /// It doesn't care about registration of components since component type present on entity is guaranteed to be registered
    /// and ignored otherwise.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    ///
    /// struct OtherComponent;
    ///
    /// let mut world = World::new();
    /// let entity = world.spawn((ExampleComponent,));
    ///
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(true));
    ///
    /// world.drop_bundle::<(ExampleComponent, OtherComponent)>(entity);
    ///
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(false));
    /// ```
    #[inline]
    pub fn drop_bundle<B>(&mut self, entity: EntityId) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        with_buffer!(self, buffer => {
            self.drop_bundle_with_buffer::<B>(entity, buffer)
        })
    }

    #[inline]
    pub(crate) fn drop_bundle_with_buffer<B>(
        &mut self,
        entity: EntityId,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        self.spawn_allocated();

        let (src_archetype, idx) = self.entities.get_location(entity).ok_or(NoSuchEntity)?;

        if B::static_with_ids(|ids| {
            ids.iter()
                .all(|&id| !self.archetypes[src_archetype as usize].has_component(id))
        }) {
            // No components to remove.
            return Ok(());
        }

        let dst_archetype = self
            .edges
            .remove_bundle::<B>(&mut self.archetypes, src_archetype);

        debug_assert_ne!(src_archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe {
            src.drop_bundle(entity, dst, idx, ActionEncoder::new(buffer, &self.entities))
        };

        self.entities
            .set_location(entity.id(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(())
    }

    /// Adds relation between two entities to the [`World`].
    ///
    /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    /// When either entity is despawned, relation is removed automatically.
    ///
    /// Relations can be queried and filtered using queries from [`edict::relation`] module.
    ///
    /// Relation must implement [`Relation`] trait that defines its behavior.
    ///
    /// If relation already exists, then instance is replaced.
    /// If relation is symmetric then it is added in both directions.
    /// If relation is exclusive, then previous relation on origin is replaced, otherwise relation is added.
    /// If relation is exclusive and symmetric, then previous relation on target is replaced, otherwise relation is added.
    #[inline]
    pub fn add_relation<R>(
        &mut self,
        entity: EntityId,
        relation: R,
        target: EntityId,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        with_buffer!(self, buffer => {
            self.add_relation_with_buffer(entity, relation, target, buffer)
        })
    }

    #[inline]
    pub(crate) fn add_relation_with_buffer<R>(
        &mut self,
        entity: EntityId,
        relation: R,
        target: EntityId,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        self.spawn_allocated();

        self.entities.get_location(entity).ok_or(NoSuchEntity)?;
        self.entities.get_location(target).ok_or(NoSuchEntity)?;

        self.epoch.next_mut();

        if R::SYMMETRIC {
            insert_component(
                self,
                entity,
                relation,
                |relation| OriginComponent::new(target, relation),
                |component, relation, encoder| component.add(entity, target, relation, encoder),
                buffer,
            );

            if target != entity {
                insert_component(
                    self,
                    target,
                    relation,
                    |relation| OriginComponent::new(entity, relation),
                    |component, relation, encoder| component.add(target, entity, relation, encoder),
                    buffer,
                );
            }
        } else {
            insert_component(
                self,
                entity,
                relation,
                |relation| OriginComponent::new(target, relation),
                |component, relation, encoder| component.add(entity, target, relation, encoder),
                buffer,
            );

            insert_component(
                self,
                target,
                (),
                |()| TargetComponent::<R>::new(entity),
                |component, (), _| component.add(entity),
                buffer,
            );
        }
        Ok(())
    }

    /// Drops relation between two entities in the [`World`].
    ///
    /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    /// If relation does not exist, does nothing.
    ///
    /// When relation is removed, [`Relation::on_drop`] behavior is not executed.
    /// For symmetric relations [`Relation::on_target_drop`] is also not executed.
    #[inline]
    pub fn remove_relation<R>(
        &mut self,
        entity: EntityId,
        target: EntityId,
    ) -> Result<R, EntityError>
    where
        R: Relation,
    {
        with_buffer!(self, buffer => {
            self.remove_relation_with_buffer::<R>(entity, target, buffer)
        })
    }

    #[inline]
    pub(crate) fn remove_relation_with_buffer<R>(
        &mut self,
        entity: EntityId,
        target: EntityId,
        buffer: &mut ActionBuffer,
    ) -> Result<R, EntityError>
    where
        R: Relation,
    {
        self.spawn_allocated();

        self.entities.get_location(entity).ok_or(NoSuchEntity)?;
        self.entities.get_location(target).ok_or(NoSuchEntity)?;

        unsafe {
            if let Ok(c) = self.query_one_unchecked::<&mut OriginComponent<R>>(entity) {
                if let Some(r) =
                    c.remove_relation(entity, target, ActionEncoder::new(buffer, &self.entities))
                {
                    return Ok(r);
                }
            }
        }
        Err(EntityError::MissingComponents)
    }

    /// Queries components from specified entity.
    /// Returns query item.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline]
    pub fn query_one_mut<'a, Q>(
        &'a mut self,
        entity: EntityId,
    ) -> Result<QueryItem<'a, Q>, QueryOneError>
    where
        Q: IntoQuery,
        Q::Query: Default,
    {
        self.query_one_with_mut(entity, Q::Query::default())
    }

    /// Queries components from specified entity.
    /// Returns query item.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline]
    pub fn query_one_with_mut<'a, Q>(
        &'a mut self,
        entity: EntityId,
        query: Q,
    ) -> Result<QueryItem<'a, Q>, QueryOneError>
    where
        Q: Query,
    {
        unsafe { self.query_one_with_unchecked::<Q>(entity, query) }
    }

    /// Queries components from specified entity.
    /// Returns query item.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline]
    pub unsafe fn query_one_unchecked<'a, Q>(
        &'a self,
        entity: EntityId,
    ) -> Result<QueryItem<'a, Q>, QueryOneError>
    where
        Q: IntoQuery,
        Q::Query: Default,
    {
        self.query_one_with_unchecked(entity, Q::Query::default())
    }

    /// Queries components from specified entity.
    /// Returns query item.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline]
    pub unsafe fn query_one_with_unchecked<'a, Q>(
        &'a self,
        entity: EntityId,
        mut query: Q,
    ) -> Result<QueryItem<'a, Q>, QueryOneError>
    where
        Q: Query,
    {
        let (archetype, idx) = self
            .entities
            .get_location(entity)
            .ok_or(QueryOneError::NoSuchEntity)?;

        let archetype = &self.archetypes[archetype as usize];

        debug_assert!(archetype.len() >= idx as usize, "Entity index is valid");

        if !query.visit_archetype(archetype) {
            return Err(QueryOneError::NotSatisfied);
        }

        let epoch = self.epoch.next();

        let mut fetch = query.fetch(archetype, epoch);

        if !fetch.visit_chunk(chunk_idx(idx as usize)) {
            return Err(QueryOneError::NotSatisfied);
        }

        fetch.touch_chunk(chunk_idx(idx as usize));

        if !fetch.visit_item(idx as usize) {
            return Err(QueryOneError::NotSatisfied);
        }

        let item = fetch.get_item(idx as usize);

        Ok(item)
    }

    /// Queries components from specified entity.
    /// Returns world borrow from which query item can be fetched.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline]
    pub fn query_one<'a, Q>(
        &'a self,
        entity: EntityId,
    ) -> Result<QueryOne<'a, Q::Query>, NoSuchEntity>
    where
        Q: IntoQuery,
        Q::Query: Default,
    {
        self.query_one_with(entity, Q::Query::default())
    }

    /// Queries components from specified entity.
    /// This method accepts query instance to support stateful queries.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline]
    pub fn query_one_with<'a, Q>(
        &'a self,
        entity: EntityId,
        query: Q,
    ) -> Result<QueryOne<'a, Q>, NoSuchEntity>
    where
        Q: Query,
    {
        let (archetype, idx) = self.entities.get_location(entity).ok_or(NoSuchEntity)?;

        let archetype = &self.archetypes[archetype as usize];

        debug_assert!(archetype.len() >= idx as usize, "Entity index is valid");

        Ok(QueryOne::new(query, archetype, idx, &self.epoch))
    }

    /// Queries components from specified entity.
    /// Calls provided closure with query item.
    /// References from query item cannot escape closure execution.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline]
    pub fn for_one<Q, F, R>(&self, entity: EntityId, f: F) -> Result<R, QueryOneError>
    where
        Q: IntoQuery,
        Q::Query: Default,
        F: for<'a> FnOnce(QueryItem<'a, Q>) -> R,
    {
        self.for_one_with(entity, Q::Query::default(), f)
    }

    /// Queries components from specified entity.
    /// Calls provided closure with query item.
    /// References from query item cannot escape closure execution.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    #[inline]
    pub fn for_one_with<Q, F, R>(
        &self,
        entity: EntityId,
        query: Q,
        f: F,
    ) -> Result<R, QueryOneError>
    where
        Q: Query,
        F: for<'a> FnOnce(QueryItem<'a, Q>) -> R,
    {
        self.query_with::<Q>(query).for_one(entity, f)
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_one_owned<Q, T>(&mut self, entity: EntityId) -> Result<T::Owned, QueryOneError>
    where
        T: ToOwned + 'static,
        Q: IntoQuery,
        Q::Query: Default + for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one::<Q, _, _>(entity, |item| T::to_owned(item))
    }

    /// Where query item is a reference to value the implements [`Clone`].
    /// Returns cloned item value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_one_cloned<Q, T>(&mut self, entity: EntityId) -> Result<T, QueryOneError>
    where
        T: Clone + 'static,
        Q: IntoQuery,
        Q::Query: Default + for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one::<Q, _, _>(entity, |item| T::clone(item))
    }
    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`Copy`].
    /// Returns copied item value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_one_copied<Q, T>(&mut self, entity: EntityId) -> Result<T, QueryOneError>
    where
        T: Copy + 'static,
        Q: IntoQuery,
        Q::Query: Default + for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one::<Q, _, _>(entity, |item| *item)
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method works only for stateless query types.
    ///
    /// Returned query can be augmented with additional sub-queries and filters.
    /// And them transformed to iterator using either [`QueryRef::iter`] or [`QueryRef::iter_mut`].
    /// Alternatively a closure may be called for each matching entity using [`QueryRef::fold`] or [`QueryRef::for_each`].
    #[inline]
    pub fn query_mut<'a, Q>(&'a mut self) -> QueryRef<'a, (Q,), ()>
    where
        Q: IntoQuery,
        Q::Query: Default,
    {
        self.query_with_mut(Q::Query::default())
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method accepts query instance to support stateful queries.
    ///
    /// Returned query can be augmented with additional sub-queries and filters.
    /// And them transformed to iterator using either [`QueryRef::iter`] or [`QueryRef::iter_mut`].
    /// Alternatively a closure may be called for each matching entity using [`QueryRef::fold`] or [`QueryRef::for_each`].
    #[inline]
    pub fn query_with_mut<'a, Q>(&'a mut self, query: Q::Query) -> QueryRef<'a, (Q,), ()>
    where
        Q: IntoQuery,
    {
        unsafe { self.query_with_unchecked(query) }
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method works only for stateless query types.
    ///
    /// Returned query can be augmented with additional sub-queries and filters.
    /// And them transformed to iterator using either [`QueryRef::iter`] or [`QueryRef::iter_mut`].
    /// Alternatively a closure may be called for each matching entity using [`QueryRef::fold`] or [`QueryRef::for_each`].
    #[inline]
    pub unsafe fn query_unchecked<'a, Q>(&'a self) -> QueryRef<'a, (Q,), ()>
    where
        Q: IntoQuery,
        Q::Query: Default,
    {
        self.query_with_unchecked(Q::Query::default())
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method accepts query instance to support stateful queries.
    ///
    /// Returned query can be augmented with additional sub-queries and filters.
    /// And them transformed to iterator using either [`QueryRef::iter`] or [`QueryRef::iter_mut`].
    /// Alternatively a closure may be called for each matching entity using [`QueryRef::fold`] or [`QueryRef::for_each`].
    #[inline]
    pub unsafe fn query_with_unchecked<'a, Q>(&'a self, query: Q::Query) -> QueryRef<'a, (Q,), ()>
    where
        Q: IntoQuery,
    {
        QueryRef::new_unchecked(self, (query,), ())
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method works only for stateless query types.
    ///
    /// Returned query can be augmented with additional sub-queries and filters.
    /// And them transformed to iterator using either [`QueryRef::iter`] or [`QueryRef::iter_mut`].
    /// Alternatively a closure may be called for each matching entity using [`QueryRef::fold`] or [`QueryRef::for_each`].
    #[inline]
    pub fn query<'a, Q>(&'a self) -> QueryRef<'a, (Q,), ()>
    where
        Q: IntoQuery,
        Q::Query: Default,
    {
        self.query_with(Q::Query::default())
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method accepts query instance to support stateful queries.
    ///
    /// Returned query can be augmented with additional sub-queries and filters.
    /// And them transformed to iterator using either [`QueryRef::iter`] or [`QueryRef::iter_mut`].
    /// Alternatively a closure may be called for each matching entity using [`QueryRef::fold`] or [`QueryRef::for_each`].
    #[inline]
    pub fn query_with<'a, Q>(&'a self, query: Q::Query) -> QueryRef<'a, (Q,), ()>
    where
        Q: IntoQuery,
    {
        QueryRef::new(self, (query,), ())
    }

    /// Starts building new query.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    #[inline]
    pub fn new_query_mut<'a>(&'a mut self) -> QueryRef<'a, (), ()> {
        unsafe { self.new_query_unchecked() }
    }

    /// Starts building new query.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    #[inline]
    pub unsafe fn new_query_unchecked<'a>(&'a self) -> QueryRef<'a, (), ()> {
        QueryRef::new_unchecked(self, (), ())
    }

    /// Starts building new query.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    #[inline]
    pub fn new_query<'a>(&'a self) -> QueryRef<'a, (), ()> {
        QueryRef::new(self, (), ())
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
    pub fn has_component<T: 'static>(&self, entity: EntityId) -> Result<bool, NoSuchEntity> {
        let (archetype, _idx) = self.entities.get_location(entity).ok_or(NoSuchEntity)?;
        Ok(self.archetypes[archetype as usize].has_component(TypeId::of::<T>()))
    }

    /// Checks if entity is alive.
    #[inline]
    pub fn is_alive(&self, entity: EntityId) -> bool {
        self.entities.get_location(entity).is_some()
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
        self.res.get_local()
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
        self.res.get_local_mut()
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

    /// Returns [`EntitySet`] from the [`World`].
    pub(crate) fn entity_set(&self) -> &EntitySet {
        &self.entities
    }

    #[inline]
    pub(crate) fn with_buffer(&mut self, buffer: &mut ActionBuffer, f: impl FnOnce(&mut World)) {
        let cached_action_buffer = self.cached_action_buffer.take();
        self.cached_action_buffer = Some(core::mem::take(buffer));
        f(self);
        *buffer = self.cached_action_buffer.take().unwrap();
        self.cached_action_buffer = cached_action_buffer;
    }

    #[inline]
    fn spawn_allocated(&mut self) {
        let epoch = self.epoch.current_mut();
        let archetype = &mut self.archetypes[0];
        self.entities
            .spawn_allocated(|id| archetype.spawn(id, (), epoch));
    }
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
pub struct SpawnBatch<'a, I> {
    bundles: I,
    epoch: EpochId,
    archetype_idx: u32,
    archetype: &'a mut Archetype,
    entities: &'a mut EntitySet,
}

impl<B, I> SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    /// Spawns the rest of the entities, dropping their ids.
    pub fn spawn_all(mut self) {
        let additional = iter_reserve_hint(&self.bundles);
        self.entities.reserve(additional);
        self.archetype.reserve(additional);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.for_each(|bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.id(), archetype_idx, idx);
        })
    }
}

impl<B, I> Iterator for SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = EntityId;

    fn next(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next()?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.id(), self.archetype_idx, idx);

        Some(entity)
    }

    fn nth(&mut self, n: usize) -> Option<EntityId> {
        // `SpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth(n)?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.id(), self.archetype_idx, idx);

        Some(entity)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, EntityId) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.entities.reserve(additional);
        self.archetype.reserve(additional);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.fold(init, |acc, bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.id(), archetype_idx, idx);
            f(acc, entity)
        })
    }

    fn collect<T>(self) -> T
    where
        T: FromIterator<EntityId>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.
        let additional = iter_reserve_hint(&self.bundles);
        self.entities.reserve(additional);
        self.archetype.reserve(additional);

        FromIterator::from_iter(self)
    }
}

impl<B, I> ExactSizeIterator for SpawnBatch<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle,
{
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<B, I> DoubleEndedIterator for SpawnBatch<'_, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next_back()?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.id(), self.archetype_idx, idx);

        Some(entity)
    }

    fn nth_back(&mut self, n: usize) -> Option<EntityId> {
        // No reason to create entities
        // for which the only reference is immediately dropped
        let bundle = self.bundles.nth_back(n)?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.id(), self.archetype_idx, idx);

        Some(entity)
    }

    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, EntityId) -> T,
    {
        self.archetype.reserve(iter_reserve_hint(&self.bundles));

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.rfold(init, |acc, bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.id(), archetype_idx, idx);
            f(acc, entity)
        })
    }
}

impl<B, I> FusedIterator for SpawnBatch<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle,
{
}

/// Error returned in case specified [`EntityId`]
/// does not reference any live entity in the [`World`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoSuchEntity;

impl fmt::Display for NoSuchEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Specified entity is not found")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NoSuchEntity {}

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
    entity: EntityId,
    value: T,
    into_component: impl FnOnce(T) -> C,
    set_component: impl FnOnce(&mut C, T, ActionEncoder),
    buffer: &mut ActionBuffer,
) where
    C: Component,
{
    let (src_archetype, idx) = world.entities.get_location(entity).unwrap();

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
        unsafe { src.insert(entity, dst, idx, component, world.epoch.current_mut()) };

    world
        .entities
        .set_location(entity.id(), dst_archetype, dst_idx);

    if let Some(src_id) = opt_src_id {
        world.entities.set_location(src_id, src_archetype, idx);
    }
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
    /// # use edict::world::{World, WorldLocal};
    ///
    pub fn get_resource<T: 'static>(&self) -> Option<Ref<T>> {
        unsafe {
            // # Safety
            // Mutable reference to `Res` ensures this is the "main" thread.
            self.world.get_local_resource()
        }
    }

    /// Returns some mutable reference to a resource.
    /// Returns none if resource is not found.
    pub fn get_resource_mut<T: 'static>(&self) -> Option<RefMut<T>> {
        unsafe {
            // # Safety
            // Mutable reference to `Res` ensures this is the "main" thread.
            self.world.get_local_resource_mut()
        }
    }
}
