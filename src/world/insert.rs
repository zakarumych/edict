use core::any::{type_name, TypeId};

use crate::{
    action::{ActionBuffer, ActionEncoder},
    bundle::{DynamicBundle, DynamicComponentBundle},
    component::{Component, ComponentInfo, ComponentRegistry},
    entity::{Entity, Location, NoSuchEntity},
};

use super::{
    assert_registered_bundle, assert_registered_one, register_bundle, register_one, World,
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
    /// let entity = world.spawn(());
    ///
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert(entity, ExampleComponent).unwrap();
    /// assert_eq!(world.has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert<T>(&mut self, entity: impl Entity, component: T) -> Result<(), NoSuchEntity>
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
        entity: impl Entity,
        component: T,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        self._insert(entity, component, register_one::<T>, buffer)
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
    pub fn insert_external<T>(
        &mut self,
        entity: impl Entity,
        component: T,
    ) -> Result<(), NoSuchEntity>
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
        entity: impl Entity,
        component: T,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        self._insert(entity, component, assert_registered_one::<T>, buffer)
    }

    pub(crate) fn _insert<T, F>(
        &mut self,
        entity: impl Entity,
        component: T,
        get_or_register: F,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        T: 'static,
        F: FnOnce(&mut ComponentRegistry) -> &ComponentInfo,
    {
        self.maintenance();

        let src_loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;
        debug_assert!(src_loc.arch < u32::MAX, "Allocated entities were spawned");

        let epoch = self.epoch.next_mut();

        let encoder = ActionEncoder::new(buffer, &self.entities);

        if self.archetypes[src_loc.arch as usize].has_component(TypeId::of::<T>()) {
            unsafe {
                self.archetypes[src_loc.arch as usize].set(
                    entity.id(),
                    src_loc.idx,
                    component,
                    epoch,
                    encoder,
                );
            }

            return Ok(());
        }

        let dst_arch = self.edges.insert(
            TypeId::of::<T>(),
            &mut self.registry,
            &mut self.archetypes,
            src_loc.arch,
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
            unsafe { src.insert(entity.id(), dst, src_loc.idx, component, epoch) };

        self.entities
            .set_location(entity.id(), Location::new(dst_arch, dst_idx));

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_loc);
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
    pub fn insert_bundle<B>(&mut self, entity: impl Entity, bundle: B) -> Result<(), NoSuchEntity>
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
        entity: impl Entity,
        bundle: B,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        self._insert_bundle(entity, bundle, register_bundle::<B>, buffer)
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
        entity: impl Entity,
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
        entity: impl Entity,
        bundle: B,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        self._insert_bundle(entity, bundle, assert_registered_bundle::<B>, buffer)
    }

    fn _insert_bundle<B, F>(
        &mut self,
        entity: impl Entity,
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

        self.maintenance();

        let src_loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;

        debug_assert!(src_loc.arch < u32::MAX, "Allocated entities were spawned");

        if bundle.with_ids(|ids| ids.is_empty()) {
            return Ok(());
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
            unsafe {
                self.archetypes[src_loc.arch as usize].set_bundle(
                    entity.id(),
                    src_loc.idx,
                    bundle,
                    epoch,
                    ActionEncoder::new(buffer, &self.entities),
                )
            }
            return Ok(());
        }

        let (before, after) = self
            .archetypes
            .split_at_mut(src_loc.arch.max(dst_arch) as usize);

        let (src, dst) = match src_loc.arch < dst_arch {
            true => (&mut before[src_loc.arch as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_arch as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe {
            src.insert_bundle(
                entity.id(),
                dst,
                src_loc.idx,
                bundle,
                epoch,
                ActionEncoder::new(buffer, &self.entities),
            )
        };

        self.entities
            .set_location(entity.id(), Location::new(dst_arch, dst_idx));

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_loc);
        }

        Ok(())
    }
}
