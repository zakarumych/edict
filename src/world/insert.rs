use core::any::type_name;

use crate::{
    action::LocalActionEncoder,
    bundle::{DynamicBundle, DynamicComponentBundle},
    component::{Component, ComponentInfo, ComponentRegistry},
    entity::{Entity, EntityLoc, Location},
    type_id, NoSuchEntity,
};

use super::{
    assert_registered_bundle, assert_registered_one, register_bundle, register_one, World,
    WorldLocal,
};

impl World {
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
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert(entity, ExampleComponent).unwrap();
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline(always)]
    pub fn insert<T>(&mut self, entity: impl Entity, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        self._with(entity, || component, true, register_one::<T>)?;
        Ok(())
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
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(false));
    /// world.ensure_external_registered::<u32>();
    /// world.insert_external(entity, 42u32).unwrap();
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline(always)]
    pub fn insert_external<T>(
        &mut self,
        entity: impl Entity,
        component: T,
    ) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        self._with(entity, || component, true, assert_registered_one::<T>)?;
        Ok(())
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
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// world.with(entity, || ExampleComponent).unwrap();
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline(always)]
    pub fn with<T>(
        &mut self,
        entity: impl Entity,
        f: impl FnOnce() -> T,
    ) -> Result<EntityLoc<'_>, NoSuchEntity>
    where
        T: Component,
    {
        self._with(entity, f, false, register_one::<T>)
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
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(false));
    /// world.ensure_external_registered::<u32>();
    /// world.with_external(entity, || 42u32).unwrap();
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline(always)]
    pub fn with_external<T>(
        &mut self,
        entity: impl Entity,
        f: impl FnOnce() -> T,
    ) -> Result<EntityLoc<'_>, NoSuchEntity>
    where
        T: 'static,
    {
        self._with(entity, f, false, assert_registered_one::<T>)
    }

    pub(crate) fn _with<T, F>(
        &mut self,
        entity: impl Entity,
        f: impl FnOnce() -> T,
        replace: bool,
        get_or_register: F,
    ) -> Result<EntityLoc<'_>, NoSuchEntity>
    where
        T: 'static,
        F: FnOnce(&mut ComponentRegistry) -> &ComponentInfo,
    {
        self.maintenance();

        let src_loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;
        debug_assert!(src_loc.arch < u32::MAX, "Allocated entities were spawned");

        let epoch = self.epoch.next_mut();

        if self.archetypes[src_loc.arch as usize].has_component(type_id::<T>()) {
            if replace {
                let encoder = LocalActionEncoder::new(self.action_buffer.get_mut(), &self.entities);
                unsafe {
                    self.archetypes[src_loc.arch as usize].set(
                        entity.id(),
                        src_loc.idx,
                        f(),
                        epoch,
                        encoder,
                    );
                }
                self.execute_local_actions();
            }

            return Ok(EntityLoc::from_parts(entity.id(), src_loc));
        }

        let dst_arch = self.edges.insert(
            &mut self.registry,
            &mut self.archetypes,
            src_loc.arch,
            type_id::<T>(),
            get_or_register,
        );

        debug_assert_ne!(src_loc.arch, dst_arch);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_loc.arch.max(dst_arch) as usize);

        let (src, dst) = match src_loc.arch < dst_arch {
            true => (&mut before[src_loc.arch as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_arch as usize]),
        };

        let (dst_idx, opt_src_id) =
            unsafe { src.insert(entity.id(), dst, src_loc.idx, f(), epoch) };

        let dst_loc = Location::new(dst_arch, dst_idx);

        self.entities.set_location(entity.id(), dst_loc);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_loc);
        }

        self.execute_local_actions();
        Ok(EntityLoc::from_parts(entity.id(), dst_loc))
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
    /// let entity = world.spawn(()).id();
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert_bundle(entity, (ExampleComponent,));
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline(always)]
    pub fn insert_bundle<B>(&mut self, entity: impl Entity, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        self._with_bundle(entity, bundle, true, register_bundle::<B>)?;
        Ok(())
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// If entity has component of any type in bundle already,
    /// it is replaced with new one.
    /// Otherwise components is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(false));
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// world.insert_external_bundle(entity, (ExampleComponent, 42u32));
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline(always)]
    pub fn insert_external_bundle<B>(
        &mut self,
        entity: impl Entity,
        bundle: B,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        self._with_bundle(entity, bundle, true, assert_registered_bundle::<B>)?;
        Ok(())
    }

    /// Inserts bundle of components to the specified entity.
    /// Adds only components missing from the entity.
    /// Components that are already present are not replaced,
    /// if replacing is required use [`World::insert_bundle`].
    ///
    /// This function guarantees that no hooks are triggered,
    /// and entity cannot be despawned as a result of this operation.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert_bundle(entity, (ExampleComponent,));
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline(always)]
    pub fn with_bundle<B>(
        &mut self,
        entity: impl Entity,
        bundle: B,
    ) -> Result<EntityLoc<'_>, NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        self._with_bundle(entity, bundle, false, register_bundle::<B>)
    }

    /// Inserts bundle of components to the specified entity.
    /// Adds only components missing from the entity.
    /// Components that are already present are not replaced,
    /// if replacing is required use [`World::insert_bundle`].
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(false));
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// world.insert_external_bundle(entity, (ExampleComponent, 42u32));
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline(always)]
    pub fn with_external_bundle<B>(
        &mut self,
        entity: impl Entity,
        bundle: B,
    ) -> Result<EntityLoc<'_>, NoSuchEntity>
    where
        B: DynamicBundle,
    {
        self._with_bundle(entity, bundle, false, assert_registered_bundle::<B>)
    }

    fn _with_bundle<B, F>(
        &mut self,
        entity: impl Entity,
        bundle: B,
        replace: bool,
        register_bundle: F,
    ) -> Result<EntityLoc<'_>, NoSuchEntity>
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

        self.maintenance();

        let src_loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;
        debug_assert!(src_loc.arch < u32::MAX, "Allocated entities were spawned");

        if bundle.with_ids(|ids| ids.is_empty()) {
            // Safety: bundle is empty, so entity is not moved
            // Reference is created with correct location of entity in this world.
            return Ok(EntityLoc::from_parts(entity.id(), src_loc));
        }

        let epoch = self.epoch.next_mut();

        let dst_arch = self.edges.insert_bundle(
            &mut self.registry,
            &mut self.archetypes,
            src_loc.arch,
            &bundle,
            |registry| register_bundle(registry, &bundle),
        );

        if dst_arch == src_loc.arch {
            if replace {
                let encoder = LocalActionEncoder::new(self.action_buffer.get_mut(), &self.entities);
                unsafe {
                    self.archetypes[src_loc.arch as usize].set_bundle(
                        entity.id(),
                        src_loc.idx,
                        bundle,
                        epoch,
                        encoder,
                    )
                }

                self.execute_local_actions();
            }

            return Ok(EntityLoc::from_parts(entity.id(), src_loc));
        }

        let (before, after) = self
            .archetypes
            .split_at_mut(src_loc.arch.max(dst_arch) as usize);

        let (src, dst) = match src_loc.arch < dst_arch {
            true => (&mut before[src_loc.arch as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_arch as usize]),
        };

        let encoder = LocalActionEncoder::new(self.action_buffer.get_mut(), &self.entities);
        let (dst_idx, opt_src_id) = unsafe {
            src.insert_bundle(
                entity.id(),
                dst,
                src_loc.idx,
                bundle,
                epoch,
                encoder,
                replace,
            )
        };

        let dst_loc = Location::new(dst_arch, dst_idx);

        self.entities.set_location(entity.id(), dst_loc);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_loc);
        }

        self.execute_local_actions();
        Ok(EntityLoc::from_parts(entity.id(), dst_loc))
    }
}

impl WorldLocal {
    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This is deferred version of [`World::insert`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::WorldLocal, ExampleComponent};
    /// let mut world = WorldLocal::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.insert_defer(entity, ExampleComponent);
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn insert_defer<T>(&self, entity: impl Entity, component: T)
    where
        T: Component,
    {
        self._with_defer(entity, || component, true, register_one::<T>)
    }

    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This is deferred version of [`World::insert_external`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::WorldLocal;
    /// let mut world = WorldLocal::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert!(!world.try_has_component::<u32>(entity).unwrap());
    ///
    /// world.ensure_external_registered::<u32>();
    /// world.insert_external_defer(entity, 42u32);
    ///
    /// assert!(!world.try_has_component::<u32>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(world.try_has_component::<u32>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn insert_external_defer<T>(&self, entity: impl Entity, component: T)
    where
        T: 'static,
    {
        self._with_defer(entity, || component, true, assert_registered_one::<T>)
    }

    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This is deferred version of [`World::with`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::WorldLocal, ExampleComponent};
    /// let mut world = WorldLocal::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.with_defer(entity, || ExampleComponent);
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn with_defer<T>(&self, entity: impl Entity, f: impl FnOnce() -> T + 'static)
    where
        T: Component,
    {
        self._with_defer(entity, f, false, register_one::<T>)
    }

    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This is deferred version of [`World::with_external`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::WorldLocal};
    /// let mut world = WorldLocal::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert!(!world.try_has_component::<u32>(entity).unwrap());
    ///
    /// world.ensure_external_registered::<u32>();
    /// world.with_external_defer(entity, || 42u32);
    ///
    /// assert!(!world.try_has_component::<u32>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(world.try_has_component::<u32>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn with_external_defer<T>(&self, entity: impl Entity, f: impl FnOnce() -> T + 'static)
    where
        T: 'static,
    {
        self._with_defer(entity, f, false, assert_registered_one::<T>)
    }

    pub(crate) fn _with_defer<T, F>(
        &self,
        entity: impl Entity,
        f: impl FnOnce() -> T + 'static,
        replace: bool,
        get_or_register: F,
    ) where
        T: 'static,
        F: FnOnce(&mut ComponentRegistry) -> &ComponentInfo + 'static,
    {
        let id = entity.id();
        self.defer(move |world| {
            let _ = world._with(id, f, replace, get_or_register);
        })
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling [`WorldLocal::insert_defer`] with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This is deferred version of [`World::insert_bundle`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::WorldLocal, ExampleComponent};
    /// let mut world = WorldLocal::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.insert_bundle_defer(entity, (ExampleComponent,));
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn insert_bundle_defer<B>(&self, entity: impl Entity, bundle: B)
    where
        B: DynamicComponentBundle + 'static,
    {
        self._with_bundle_defer(entity, bundle, true, register_bundle::<B>);
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling [`WorldLocal::insert_defer`] with each component separately,
    /// but more efficient.
    ///
    /// If entity has component of any type in bundle already,
    /// it is replaced with new one.
    /// Otherwise components is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This is deferred version of [`World::insert_external_bundle`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::WorldLocal, ExampleComponent};
    /// let mut world = WorldLocal::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// assert!(!world.try_has_component::<u32>(entity).unwrap());
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// world.insert_external_bundle_defer(entity, (ExampleComponent, 42u32));
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// assert!(!world.try_has_component::<u32>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// assert!(world.try_has_component::<u32>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn insert_external_bundle_defer<B>(&self, entity: impl Entity, bundle: B)
    where
        B: DynamicBundle + 'static,
    {
        self._with_bundle_defer(entity, bundle, true, assert_registered_bundle::<B>);
    }

    /// Inserts bundle of components to the specified entity.
    /// Adds only components missing from the entity.
    /// Components that are already present are not replaced,
    /// if replacing is required use [`WorldLocal::insert_bundle_defer`].
    ///
    /// This function guarantees that no hooks are triggered,
    /// and entity cannot be despawned as a result of this operation.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This is deferred version of [`World::with_bundle`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::WorldLocal, ExampleComponent};
    /// let mut world = WorldLocal::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.with_bundle_defer(entity, (ExampleComponent,));
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn with_bundle_defer<B>(&self, entity: impl Entity, bundle: B)
    where
        B: DynamicComponentBundle + 'static,
    {
        self._with_bundle_defer(entity, bundle, false, register_bundle::<B>);
    }

    /// Inserts bundle of components to the specified entity.
    /// Adds only components missing from the entity.
    /// Components that are already present are not replaced,
    /// if replacing is required use [`World::insert_bundle`].
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// This is deferred version of [`World::with_external_bundle`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::WorldLocal, ExampleComponent};
    /// let mut world = WorldLocal::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// assert!(!world.try_has_component::<u32>(entity).unwrap());
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// world.with_external_bundle_defer(entity, (ExampleComponent, 42u32));
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// assert!(!world.try_has_component::<u32>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// assert!(world.try_has_component::<u32>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn with_external_bundle_defer<B>(&self, entity: impl Entity, bundle: B)
    where
        B: DynamicBundle + 'static,
    {
        self._with_bundle_defer(entity, bundle, false, assert_registered_bundle::<B>);
    }

    fn _with_bundle_defer<B, F>(
        &self,
        entity: impl Entity,
        bundle: B,
        replace: bool,
        register_bundle: F,
    ) where
        B: DynamicBundle + 'static,
        F: FnOnce(&mut ComponentRegistry, &B) + 'static,
    {
        let id = entity.id();
        self.defer(move |world| {
            let _ = world._with_bundle(id, bundle, replace, register_bundle);
        });
    }
}
