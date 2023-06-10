impl World {
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
        origin: EntityId,
        relation: R,
        target: EntityId,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        with_buffer!(self, buffer => {
            self.add_relation_with_buffer(origin, relation, target, buffer)
        })
    }

    #[inline]
    pub(crate) fn add_relation_with_buffer<R>(
        &mut self,
        origin: EntityId,
        relation: R,
        target: EntityId,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        self.maintenance();

        self.entities.get_location(origin).ok_or(NoSuchEntity)?;
        self.entities.get_location(target).ok_or(NoSuchEntity)?;

        self.epoch.next_mut();

        if R::SYMMETRIC {
            set_relation_component(
                self,
                origin,
                relation,
                |relation| OriginComponent::new(target, relation),
                |component, relation, encoder| component.add(origin, target, relation, encoder),
                buffer,
            );

            if target != origin {
                set_relation_component(
                    self,
                    target,
                    relation,
                    |relation| OriginComponent::new(origin, relation),
                    |component, relation, encoder| component.add(target, origin, relation, encoder),
                    buffer,
                );
            }
        } else {
            set_relation_component(
                self,
                origin,
                relation,
                |relation| OriginComponent::new(target, relation),
                |component, relation, encoder| component.add(origin, target, relation, encoder),
                buffer,
            );

            set_relation_component(
                self,
                target,
                (),
                |()| TargetComponent::<R>::new(origin),
                |component, (), _| component.add(origin),
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
        origin: EntityId,
        target: EntityId,
    ) -> Result<R, EntityError>
    where
        R: Relation,
    {
        with_buffer!(self, buffer => {
            self.remove_relation_with_buffer::<R>(origin, target, buffer)
        })
    }

    #[inline]
    pub(crate) fn remove_relation_with_buffer<R>(
        &mut self,
        origin: EntityId,
        target: EntityId,
        buffer: &mut ActionBuffer,
    ) -> Result<R, EntityError>
    where
        R: Relation,
    {
        self.maintenance();

        self.entities.get_location(origin).ok_or(NoSuchEntity)?;
        self.entities.get_location(target).ok_or(NoSuchEntity)?;

        unsafe {
            if let Ok(c) = self.query_one_unchecked::<&mut OriginComponent<R>>(origin) {
                if let Some(r) =
                    c.remove_relation(origin, target, ActionEncoder::new(buffer, &self.entities))
                {
                    return Ok(r);
                }
            }
        }
        Err(EntityError::MissingComponents)
    }
}

/// Inserts component.
/// This function uses different code to assign component when it already exists on entity.
fn set_relation_component<T, C>(
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
