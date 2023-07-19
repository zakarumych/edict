use core::{any::type_name, marker::PhantomData};

use crate::{
    action::{ActionBuffer, ActionEncoder},
    archetype::Archetype,
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle},
    component::ComponentRegistry,
    entity::{Entity, EntityId, EntityLoc, EntityRef, EntitySet, Location},
    epoch::EpochId,
    NoSuchEntity,
};

use super::{assert_registered_bundle, register_bundle, World};

/// Limits on reserving of space for entities and components
/// in archetypes when `spawn_batch` is used.
const MAX_SPAWN_RESERVE: usize = 1024;

impl World {
    /// Reserves new entity.
    ///
    /// The entity will be materialized before first mutation on the world happens.
    /// Until then entity is alive and belongs to a dummy archetype.
    /// Entity will be alive until despawned.
    ///
    /// # Panics
    ///
    /// Panics if new id cannot be allocated.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let entity = world.allocate().id();
    /// assert!(world.is_alive(entity));
    /// world.despawn(entity).unwrap();
    /// assert!(!world.is_alive(entity));
    /// ```
    #[inline(always)]
    pub fn allocate(&self) -> EntityLoc<'_> {
        self.entities.alloc()
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// Returns [`EntityId`] to the newly spawned entity.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`World::despawn`] is called with returned [`EntityId`].
    ///
    /// # Panics
    ///
    /// Panics if new id cannot be allocated.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent};
    /// let mut world = World::new();
    /// let mut entity = world.spawn((ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline(always)]
    pub fn spawn<B>(&mut self, bundle: B) -> EntityRef<'_>
    where
        B: DynamicComponentBundle,
    {
        self.maintenance();
        self._spawn(bundle, register_bundle::<B>)
    }

    /// Spawns a new entity in this world with specific ID and bundle of components.
    /// The `World` must be configured to never allocate this ID.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`World::despawn`] is called with the same [`EntityId`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent, EntityId};
    /// let mut world = World::new();
    /// let id = EntityId::from_bits(42).unwrap();
    /// let mut entity = world.spawn_at(id, (ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline(always)]
    pub fn spawn_at<B>(&mut self, id: EntityId, bundle: B) -> EntityRef<'_>
    where
        B: DynamicComponentBundle,
    {
        self.maintenance();
        let (spawned, entity) = self._spawn_at(id, bundle, register_bundle::<B>);
        assert!(spawned);
        entity
    }

    /// Spawns a new entity in this world with specific ID and bundle of components.
    /// The `World` must be configured to never allocate this ID.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`World::despawn`] is called with the same [`EntityId`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent, EntityId};
    /// let mut world = World::new();
    /// let id = EntityId::from_bits(42).unwrap();
    /// let mut entity = world.spawn_or_insert(id, (ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline(always)]
    pub fn spawn_or_insert<B>(&mut self, id: EntityId, bundle: B) -> EntityRef<'_>
    where
        B: DynamicComponentBundle,
    {
        self.maintenance();
        let (_spawned, entity) = self._spawn_at(id, bundle, register_bundle::<B>);
        entity
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// Returns [`EntityRef`] handle to the newly spawned entity.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until despawned.
    ///
    /// All components from the bundle must be previously registered.
    /// If component in bundle implements [`Component`] it could be registered implicitly
    /// on first by [`World::spawn`], [`World::spawn_batch`], [`World::insert`] or [`World::insert_bundle`].
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`] or later by [`World::ensure_component_registered`].
    /// Non [`Component`] types must be pre-registered by [`WorldBuilder::register_external`] or later by [`World::ensure_external_registered`].
    ///
    /// # Panics
    ///
    /// Panics if new id cannot be allocated.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent};
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    /// world.ensure_component_registered::<ExampleComponent>();
    /// let mut entity = world.spawn_external((42u32, ExampleComponent));
    /// assert!(entity.has_component::<u32>());
    /// assert_eq!(entity.remove(), Some(42u32));
    /// assert!(!entity.has_component::<u32>());
    /// ```
    #[inline(always)]
    pub fn spawn_external<B>(&mut self, bundle: B) -> EntityRef<'_>
    where
        B: DynamicBundle,
    {
        self.maintenance();
        self._spawn(bundle, assert_registered_bundle::<B>)
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// The id must be unused by the world.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until despawned.
    ///
    /// All components from the bundle must be previously registered.
    /// If component in bundle implements [`Component`] it could be registered implicitly
    /// on first by [`World::spawn`], [`World::spawn_batch`], [`World::insert`] or [`World::insert_bundle`].
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`] or later by [`World::ensure_component_registered`].
    /// Non [`Component`] types must be pre-registered by [`WorldBuilder::register_external`] or later by [`World::ensure_external_registered`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent};
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    /// world.ensure_component_registered::<ExampleComponent>();
    /// let mut entity = world.spawn_external((42u32, ExampleComponent));
    /// assert!(entity.has_component::<u32>());
    /// assert_eq!(entity.remove(), Some(42u32));
    /// assert!(!entity.has_component::<u32>());
    /// ```
    #[inline(always)]
    pub fn spawn_external_at<B>(&mut self, id: EntityId, bundle: B) -> EntityRef<'_>
    where
        B: DynamicBundle,
    {
        self.maintenance();
        let (spawned, entity) = self._spawn_at(id, bundle, assert_registered_bundle::<B>);
        assert!(spawned);
        entity
    }

    /// Umbrella method for spawning entity with new ID.
    fn _spawn<B, F>(&mut self, bundle: B, register_bundle: F) -> EntityRef<'_>
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

        let arch_idx = self.edges.spawn(
            &mut self.registry,
            &mut self.archetypes,
            &bundle,
            |registry| register_bundle(registry, &bundle),
        );

        let epoch = self.epoch.next_mut();
        let (id, loc) = self.entities.spawn(arch_idx, |id| {
            self.archetypes[arch_idx as usize].spawn(id, bundle, epoch)
        });

        unsafe { EntityRef::from_parts(id, loc, self) }
    }

    /// Umbrella method for spawning entity with existing ID.
    /// Returns tuple of boolean flag indicating if entity was actually spawned
    /// and [`EntityRef`] handle to the newly spawned entity.
    ///
    /// If entity is not spawned, bundle is dropped.
    fn _spawn_at<B, F>(
        &mut self,
        id: EntityId,
        bundle: B,
        register_bundle: F,
    ) -> (bool, EntityRef<'_>)
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

        let arch_idx = self.edges.spawn(
            &mut self.registry,
            &mut self.archetypes,
            &bundle,
            |registry| register_bundle(registry, &bundle),
        );

        let epoch = self.epoch.next_mut();
        let (spawned, loc) = self.entities.spawn_at(id, arch_idx, || {
            self.archetypes[arch_idx as usize].spawn(id, bundle, epoch)
        });

        (spawned, unsafe { EntityRef::from_parts(id, loc, self) })
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
    #[inline(always)]
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
    #[inline(always)]
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

        self.maintenance();

        let arch_idx = self.edges.insert_bundle(
            &mut self.registry,
            &mut self.archetypes,
            0,
            &PhantomData::<I::Item>,
            register_bundle,
        );

        let epoch = self.epoch.next_mut();

        let archetype = &mut self.archetypes[arch_idx as usize];
        let entities = &mut self.entities;

        SpawnBatch {
            bundles: bundles.into_iter(),
            epoch,
            arch_idx,
            archetype,
            entities,
        }
    }

    pub(crate) fn spawn_reserve<B>(&mut self, additional: u32)
    where
        B: Bundle,
    {
        self.entities.reserve(additional);

        let arch_idx = self.edges.insert_bundle(
            &mut self.registry,
            &mut self.archetypes,
            0,
            &PhantomData::<B>,
            |registry| assert_registered_bundle(registry, &PhantomData::<B>),
        );

        let archetype = &mut self.archetypes[arch_idx as usize];
        archetype.reserve(additional);
    }

    /// Despawns an entity with specified id.
    /// Returns [`Err(NoSuchEntity)`] if entity does not exists.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn((ExampleComponent,)).id();
    /// assert!(world.despawn(entity).is_ok(), "Entity should be despawned by this call");
    /// assert!(world.despawn(entity).is_err(), "Already despawned");
    /// ```
    #[inline(always)]
    pub fn despawn(&mut self, entity: impl Entity) -> Result<(), NoSuchEntity> {
        with_buffer!(self, buffer => self.despawn_with_buffer(entity, buffer))
    }

    #[inline(always)]
    pub(crate) fn despawn_with_buffer(
        &mut self,
        entity: impl Entity,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity> {
        self.maintenance();

        let loc = self.entities.despawn(entity.id()).ok_or(NoSuchEntity)?;

        let encoder = ActionEncoder::new(buffer, &self.entities);
        let opt_id = unsafe {
            self.archetypes[loc.arch as usize].despawn_unchecked(entity.id(), loc.idx, encoder)
        };

        if let Some(id) = opt_id {
            self.entities.set_location(id, loc)
        }

        Ok(())
    }

    /// Special-case despawn method for [`EntityRef::despawn`].
    /// This method uses branch elimination for non-existent entity case
    /// and prevents data dependencies between removing entity from
    /// `EntitySet` and `Archetype`.
    #[inline(always)]
    pub(crate) unsafe fn despawn_ref(&mut self, id: EntityId, loc: Location) {
        with_buffer!(self, buffer => {
            let real_loc = self.entities.despawn(id).unwrap_unchecked();
            debug_assert_eq!(real_loc, loc, "Entity location mismatch");

            let opt_id = unsafe {
                self.archetypes[loc.arch as usize].despawn_unchecked(id, loc.idx, ActionEncoder::new(buffer, &self.entities))
            };

            if let Some(id) = opt_id {
                self.entities
                    .set_location(id, loc)
            }
        })
    }
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
pub struct SpawnBatch<'a, I> {
    bundles: I,
    epoch: EpochId,
    arch_idx: u32,
    archetype: &'a mut Archetype,
    entities: &'a mut EntitySet,
}

impl<B, I> SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    /// Spawns the rest of the entities.
    /// The bundles iterator will be exhausted.
    /// If bundles iterator is fused, calling this method again will
    /// never spawn entities.
    ///
    /// This method won't return IDs of spawned entities.
    #[inline]
    pub fn spawn_all(&mut self) {
        let additional = iter_reserve_hint(&self.bundles);
        self.entities.reserve(additional);
        self.archetype.reserve(additional);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let arch_idx = self.arch_idx;
        let epoch = self.epoch;

        self.bundles.by_ref().for_each(|bundle| {
            entities.spawn(arch_idx, |id| archetype.spawn(id, bundle, epoch));
        })
    }
}

impl<'a, B, I> Iterator for SpawnBatch<'a, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = EntityLoc<'a>;

    #[inline(always)]
    fn next(&mut self) -> Option<EntityLoc<'a>> {
        let bundle = self.bundles.next()?;

        let (id, loc) = self.entities.spawn(self.arch_idx, |id| {
            self.archetype.spawn(id, bundle, self.epoch)
        });
        Some(EntityLoc::new(id, loc))
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<EntityLoc<'a>> {
        // `SpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth(n)?;

        let (id, loc) = self.entities.spawn(self.arch_idx, |id| {
            self.archetype.spawn(id, bundle, self.epoch)
        });

        Some(EntityLoc::new(id, loc))
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    #[inline]
    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, EntityLoc<'a>) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.entities.reserve(additional);
        self.archetype.reserve(additional);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let arch_idx = self.arch_idx;
        let epoch = self.epoch;

        self.bundles.fold(init, |acc, bundle| {
            let (id, loc) = entities.spawn(arch_idx, |id| archetype.spawn(id, bundle, epoch));
            f(acc, EntityLoc::new(id, loc))
        })
    }

    #[inline(always)]
    fn collect<T>(self) -> T
    where
        T: FromIterator<EntityLoc<'a>>,
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
    #[inline(always)]
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<'a, B, I> DoubleEndedIterator for SpawnBatch<'a, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<EntityLoc<'a>> {
        let bundle = self.bundles.next_back()?;

        let (id, loc) = self.entities.spawn(self.arch_idx, |id| {
            self.archetype.spawn(id, bundle, self.epoch)
        });
        Some(EntityLoc::new(id, loc))
    }

    fn nth_back(&mut self, n: usize) -> Option<EntityLoc<'a>> {
        // No reason to create entities
        // for which the only reference is immediately dropped
        let bundle = self.bundles.nth_back(n)?;

        let (id, loc) = self.entities.spawn(self.arch_idx, |id| {
            self.archetype.spawn(id, bundle, self.epoch)
        });
        Some(EntityLoc::new(id, loc))
    }

    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, EntityLoc<'a>) -> T,
    {
        self.archetype.reserve(iter_reserve_hint(&self.bundles));

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let arch_idx = self.arch_idx;
        let epoch = self.epoch;

        self.bundles.rfold(init, |acc, bundle| {
            let (id, loc) = entities.spawn(arch_idx, |id| archetype.spawn(id, bundle, epoch));
            f(acc, EntityLoc::new(id, loc))
        })
    }
}

impl<B, I> core::iter::FusedIterator for SpawnBatch<'_, I>
where
    I: core::iter::FusedIterator<Item = B>,
    B: Bundle,
{
}

pub(crate) fn iter_reserve_hint(iter: &impl Iterator) -> u32 {
    let (lower, upper) = iter.size_hint();
    match (lower, upper) {
        (lower, None) => lower.min(u32::MAX as usize) as u32,
        (lower, Some(upper)) => {
            // Iterator is consumed in full, so reserve at least `lower`.
            lower
                .max(upper.min(MAX_SPAWN_RESERVE))
                .min(u32::MAX as usize) as u32
        }
    }
}
