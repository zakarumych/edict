use core::any::{type_name, TypeId};

use crate::{
    action::LocalActionEncoder,
    bundle::Bundle,
    entity::{Entity, EntityRef, Location},
    type_id, NoSuchEntity,
};

use super::{World, WorldLocal};

impl World {
    /// Removes component from the specified entity and returns its value.
    ///
    /// Returns `Ok(Some(comp))` if component was removed.
    /// Returns `Ok(None)` if entity does not have component of this type.
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    #[inline(always)]
    pub fn remove<T>(
        &mut self,
        entity: impl Entity,
    ) -> Result<(Option<T>, EntityRef<'_>), NoSuchEntity>
    where
        T: 'static,
    {
        self.maintenance();

        let src_loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;
        debug_assert!(src_loc.arch < u32::MAX, "Allocated entities were spawned");

        if !self.archetypes[src_loc.arch as usize].has_component(type_id::<T>()) {
            // Safety: entity is not moved
            // Reference is created with correct location of entity in this world.
            let e = unsafe { EntityRef::from_parts(entity.id(), src_loc, self.local()) };
            return Ok((None, e));
        }

        let dst_arch = self
            .edges
            .remove(&mut self.archetypes, src_loc.arch, type_id::<T>());

        debug_assert_ne!(src_loc.arch, dst_arch);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_loc.arch.max(dst_arch) as usize);

        let (src, dst) = match src_loc.arch < dst_arch {
            true => (&mut before[src_loc.arch as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_arch as usize]),
        };

        let (dst_idx, opt_src_id, component) =
            unsafe { src.remove::<T>(entity.id(), dst, src_loc.idx) };

        let dst_loc = Location::new(dst_arch, dst_idx);

        self.entities.set_location(entity.id(), dst_loc);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_loc);
        }

        // Safety: entity is moved
        // Reference is created with correct location of entity in this world.
        let e = unsafe { EntityRef::from_parts(entity.id(), dst_loc, self.local()) };

        Ok((Some(component), e))
    }

    /// Drops component from the specified entity.
    ///
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    #[inline(always)]
    pub fn drop<T>(&mut self, entity: impl Entity) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        self.drop_erased(entity, type_id::<T>())
    }

    /// Drops component from the specified entity.
    ///
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    #[inline(always)]
    pub fn drop_erased(&mut self, entity: impl Entity, ty: TypeId) -> Result<(), NoSuchEntity> {
        self.maintenance();

        let src_loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;
        debug_assert!(src_loc.arch < u32::MAX, "Allocated entities were spawned");

        if !self.archetypes[src_loc.arch as usize].has_component(ty) {
            // Safety: entity is not moved
            // Reference is created with correct location of entity in this world.
            return Ok(());
        }

        let dst_arch = self.edges.remove(&mut self.archetypes, src_loc.arch, ty);

        debug_assert_ne!(src_loc.arch, dst_arch);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_loc.arch.max(dst_arch) as usize);

        let (src, dst) = match src_loc.arch < dst_arch {
            true => (&mut before[src_loc.arch as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_arch as usize]),
        };

        let encoder = LocalActionEncoder::new(self.action_buffer.get_mut(), &self.entities);
        let (dst_idx, opt_src_id) =
            unsafe { src.drop_bundle(entity.id(), dst, src_loc.idx, encoder) };

        let dst_loc = Location::new(dst_arch, dst_idx);

        self.entities.set_location(entity.id(), dst_loc);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_loc);
        }

        self.execute_local_actions();
        Ok(())
    }

    /// Drops entity's components that are found in the specified bundle.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// Unlike other methods that use `Bundle` trait, this method does not require
    /// all components from bundle to be registered in the world.
    /// Entity can't have components that are not registered in the world,
    /// so no need to drop them.
    ///
    /// For this reason there's no separate method that uses `ComponentBundle` trait.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    ///
    /// struct OtherComponent;
    ///
    /// let mut world = World::new();
    /// let mut entity = world.spawn((ExampleComponent,)).id();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// world.drop_bundle::<(ExampleComponent, OtherComponent)>(entity).unwrap();
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn drop_bundle<B>(&mut self, entity: impl Entity) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        self.maintenance();

        let src_loc = entity.lookup(&self.entities).ok_or(NoSuchEntity)?;
        debug_assert!(src_loc.arch < u32::MAX, "Allocated entities were spawned");

        if B::static_with_ids(|ids| {
            ids.iter()
                .all(|&id| !self.archetypes[src_loc.arch as usize].has_component(id))
        }) {
            // Safety: entity is not moved
            // Reference is created with correct location of entity in this world.
            return Ok(());
        }

        let dst_arch = self
            .edges
            .remove_bundle::<B>(&mut self.archetypes, src_loc.arch);

        debug_assert_ne!(src_loc.arch, dst_arch);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_loc.arch.max(dst_arch) as usize);

        let (src, dst) = match src_loc.arch < dst_arch {
            true => (&mut before[src_loc.arch as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_arch as usize]),
        };

        let encoder = LocalActionEncoder::new(self.action_buffer.get_mut(), &self.entities);
        let (dst_idx, opt_src_id) =
            unsafe { src.drop_bundle(entity.id(), dst, src_loc.idx, encoder) };

        let dst_loc = Location::new(dst_arch, dst_idx);

        self.entities.set_location(entity.id(), dst_loc);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_loc);
        }

        self.execute_local_actions();
        Ok(())
    }
}

impl WorldLocal {
    /// Drops component from the specified entity.
    ///
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    ///
    /// This is deferred version of [`World::drop`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    ///
    /// let mut world = World::new();
    /// let world = world.local();;
    /// let mut entity = world.spawn((ExampleComponent,)).id();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.drop_defer::<ExampleComponent>(entity);
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn drop_defer<T>(&self, entity: impl Entity)
    where
        T: 'static,
    {
        self.drop_erased_defer(entity, type_id::<T>())
    }

    /// Drops component from the specified entity.
    ///
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    ///
    /// This is deferred version of [`World::drop_erased`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    #[inline(always)]
    pub fn drop_erased_defer(&self, entity: impl Entity, ty: TypeId) {
        let id = entity.id();
        self.defer(move |world| {
            let _ = world.drop_erased(id, ty);
        })
    }

    /// Drops entity's components that are found in the specified bundle.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// Unlike other methods that use `Bundle` trait, this method does not require
    /// all components from bundle to be registered in the world.
    /// Entity can't have components that are not registered in the world,
    /// so no need to drop them.
    ///
    /// For this reason there's no separate method that uses `ComponentBundle` trait.
    ///
    /// This is deferred version of [`World::drop_bundle`].
    /// It can be used on shared `WorldLocal` reference.
    /// Operation is queued and executed on next call to [`World::run_deferred`]
    /// or when mutable operation is performed on the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    ///
    /// struct OtherComponent;
    ///
    /// let mut world = World::new();
    /// let world = world.local();;
    /// let mut entity = world.spawn((ExampleComponent,)).id();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.drop_bundle_defer::<(ExampleComponent, OtherComponent)>(entity);
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    ///
    /// world.run_deferred();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// ```
    #[inline(always)]
    pub fn drop_bundle_defer<B>(&self, entity: impl Entity)
    where
        B: Bundle,
    {
        let id = entity.id();
        self.defer(move |world| {
            let _ = world.drop_bundle::<B>(id);
        });
    }
}
