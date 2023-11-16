use core::any::TypeId;

use crate::{
    action::{ActionBuffer, ActionEncoder},
    component::Component,
    entity::{Entity, EntityId, Location},
    relation::{OriginComponent, Relation, TargetComponent},
    NoSuchEntity,
};

use super::World;

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
    #[inline(always)]
    pub fn add_relation<R>(
        &mut self,
        origin: impl Entity,
        relation: R,
        target: impl Entity,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        with_buffer!(self, buffer => {
            self.add_relation_with_buffer(origin, relation, target, buffer)
        })
    }

    #[inline(always)]
    pub(crate) fn add_relation_with_buffer<R>(
        &mut self,
        origin: impl Entity,
        relation: R,
        target: impl Entity,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        self.maintenance();

        origin.lookup(&self.entities).ok_or(NoSuchEntity)?;
        target.lookup(&self.entities).ok_or(NoSuchEntity)?;

        self.epoch.next_mut();

        if R::SYMMETRIC {
            set_relation_component(
                self,
                origin.id(),
                relation,
                |relation| OriginComponent::new_relation(target.id(), relation),
                |component, relation, encoder| {
                    component.add_relation(origin.id(), target.id(), relation, encoder)
                },
                buffer,
            );

            if target.id() != origin.id() {
                set_relation_component(
                    self,
                    target.id(),
                    relation,
                    |relation| OriginComponent::new_relation(origin.id(), relation),
                    |component, relation, encoder| {
                        component.add_relation(target.id(), origin.id(), relation, encoder)
                    },
                    buffer,
                );
            }
        } else {
            set_relation_component(
                self,
                origin.id(),
                relation,
                |relation| OriginComponent::new_relation(target.id(), relation),
                |comp, relation, encoder| {
                    comp.add_relation(origin.id(), target.id(), relation, encoder)
                },
                buffer,
            );

            set_relation_component(
                self,
                target.id(),
                (),
                |()| TargetComponent::<R>::new(origin.id()),
                |comp, (), _| comp.add(origin.id()),
                buffer,
            );
        }
        Ok(())
    }

    /// Removes relation between two entities in the [`World`].
    ///
    /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    /// If relation does not exist, removes `None`.
    ///
    /// When relation is removed, [`Relation::on_drop`] behavior is not executed.
    /// For symmetric relations [`Relation::on_target_drop`] is also not executed.
    #[inline(always)]
    pub fn remove_relation<R>(
        &mut self,
        origin: impl Entity,
        target: impl Entity,
    ) -> Result<Option<R>, NoSuchEntity>
    where
        R: Relation,
    {
        with_buffer!(self, buffer => {
            self.remove_relation_with_buffer::<R>(origin, target, buffer)
        })
    }

    #[inline(always)]
    pub(crate) fn remove_relation_with_buffer<R>(
        &mut self,
        origin: impl Entity,
        target: impl Entity,
        buffer: &mut ActionBuffer,
    ) -> Result<Option<R>, NoSuchEntity>
    where
        R: Relation,
    {
        self._remove_relation(origin, target, buffer, |_, _, _, _| {})
    }

    /// Drops relation between two entities in the [`World`].
    ///
    /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    /// If relation does not exist, does nothing.
    ///
    /// When relation is dropped, [`Relation::on_drop`] behavior is executed.
    #[inline(always)]
    pub fn drop_relation<R>(
        &mut self,
        origin: impl Entity,
        target: impl Entity,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        with_buffer!(self, buffer => {
            self.drop_relation_with_buffer::<R>(origin, target, buffer)
        })
    }

    #[inline(always)]
    pub(crate) fn drop_relation_with_buffer<R>(
        &mut self,
        origin: impl Entity,
        target: impl Entity,
        buffer: &mut ActionBuffer,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        self._remove_relation(origin, target, buffer, R::on_drop)?;
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn _remove_relation<R>(
        &mut self,
        origin: impl Entity,
        target: impl Entity,
        buffer: &mut ActionBuffer,
        on_drop: impl FnOnce(&mut R, EntityId, EntityId, ActionEncoder<'_>),
    ) -> Result<Option<R>, NoSuchEntity>
    where
        R: Relation,
    {
        self.maintenance();

        let origin = origin.entity_loc(&self.entities).ok_or(NoSuchEntity)?;
        let target = target.entity_loc(&self.entities).ok_or(NoSuchEntity)?;

        unsafe {
            if let Ok(comp) = self.get_unchecked::<&mut OriginComponent<R>>(origin) {
                if let Some(mut relation) = comp.remove_relation(
                    origin.id(),
                    target.id(),
                    ActionEncoder::new(buffer, &self.entities),
                ) {
                    on_drop(
                        &mut relation,
                        origin.id(),
                        target.id(),
                        ActionEncoder::new(buffer, &self.entities),
                    );

                    if R::SYMMETRIC {
                        if origin.id() != target.id() {
                            let comp = self
                                .get_unchecked::<&mut OriginComponent<R>>(target)
                                .unwrap_unchecked();

                            comp.remove_relation(
                                target.id(),
                                origin.id(),
                                ActionEncoder::new(buffer, &self.entities),
                            );
                        }
                    } else {
                        let comp = self
                            .get_unchecked::<&mut TargetComponent<R>>(target)
                            .unwrap_unchecked();

                        comp.remove_relation(
                            origin.id(),
                            target.id(),
                            ActionEncoder::new(buffer, &self.entities),
                        );
                    }

                    return Ok(Some(relation));
                }
            }
        }
        Ok(None)
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
    let src_loc = world.entities.get_location(id).unwrap();
    debug_assert!(src_loc.arch < u32::MAX, "Allocated entities were spawned");

    if world.archetypes[src_loc.arch as usize].has_component(TypeId::of::<C>()) {
        let component = unsafe {
            world.archetypes[src_loc.arch as usize]
                .get_mut::<C>(src_loc.idx, world.epoch.current_mut())
        };

        set_component(
            component,
            value,
            ActionEncoder::new(buffer, &world.entities),
        );

        return;
    }

    let component = into_component(value);

    let dst_arch = world.edges.insert(
        &mut world.registry,
        &mut world.archetypes,
        src_loc.arch,
        TypeId::of::<C>(),
        |registry| registry.get_or_register::<C>(),
    );

    debug_assert_ne!(src_loc.arch, dst_arch);

    let (before, after) = world
        .archetypes
        .split_at_mut(src_loc.arch.max(dst_arch) as usize);

    let (src, dst) = match src_loc.arch < dst_arch {
        true => (&mut before[src_loc.arch as usize], &mut after[0]),
        false => (&mut after[0], &mut before[dst_arch as usize]),
    };

    let (dst_idx, opt_src_id) =
        unsafe { src.insert(id, dst, src_loc.idx, component, world.epoch.current_mut()) };

    world
        .entities
        .set_location(id, Location::new(dst_arch, dst_idx));

    if let Some(src_id) = opt_src_id {
        world.entities.set_location(src_id, src_loc);
    }
}
