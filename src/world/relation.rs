impl World {
    // /// Adds relation between two entities to the [`World`].
    // ///
    // /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    // /// When either entity is despawned, relation is removed automatically.
    // ///
    // /// Relations can be queried and filtered using queries from [`edict::relation`] module.
    // ///
    // /// Relation must implement [`Relation`] trait that defines its behavior.
    // ///
    // /// If relation already exists, then instance is replaced.
    // /// If relation is symmetric then it is added in both directions.
    // /// If relation is exclusive, then previous relation on origin is replaced, otherwise relation is added.
    // /// If relation is exclusive and symmetric, then previous relation on target is replaced, otherwise relation is added.
    // #[inline]
    // pub fn add_relation<R>(
    //     &mut self,
    //     origin: EntityId,
    //     relation: R,
    //     target: EntityId,
    // ) -> Result<(), NoSuchEntity>
    // where
    //     R: Relation,
    // {
    //     with_buffer!(self, buffer => {
    //         self.add_relation_with_buffer(origin, relation, target, buffer)
    //     })
    // }

    // #[inline]
    // pub(crate) fn add_relation_with_buffer<R>(
    //     &mut self,
    //     origin: EntityId,
    //     relation: R,
    //     target: EntityId,
    //     buffer: &mut ActionBuffer,
    // ) -> Result<(), NoSuchEntity>
    // where
    //     R: Relation,
    // {
    //     self.maintenance();

    //     self.entities.get_location(origin).ok_or(NoSuchEntity)?;
    //     self.entities.get_location(target).ok_or(NoSuchEntity)?;

    //     self.epoch.next_mut();

    //     if R::SYMMETRIC {
    //         insert_component(
    //             self,
    //             origin,
    //             relation,
    //             |relation| OriginComponent::new(target, relation),
    //             |component, relation, encoder| component.add(origin, target, relation, encoder),
    //             buffer,
    //         );

    //         if target != origin {
    //             insert_component(
    //                 self,
    //                 target,
    //                 relation,
    //                 |relation| OriginComponent::new(origin, relation),
    //                 |component, relation, encoder| component.add(target, origin, relation, encoder),
    //                 buffer,
    //             );
    //         }
    //     } else {
    //         insert_component(
    //             self,
    //             origin,
    //             relation,
    //             |relation| OriginComponent::new(target, relation),
    //             |component, relation, encoder| component.add(origin, target, relation, encoder),
    //             buffer,
    //         );

    //         insert_component(
    //             self,
    //             target,
    //             (),
    //             |()| TargetComponent::<R>::new(origin),
    //             |component, (), _| component.add(origin),
    //             buffer,
    //         );
    //     }
    //     Ok(())
    // }

    // /// Drops relation between two entities in the [`World`].
    // ///
    // /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    // /// If relation does not exist, does nothing.
    // ///
    // /// When relation is removed, [`Relation::on_drop`] behavior is not executed.
    // /// For symmetric relations [`Relation::on_target_drop`] is also not executed.
    // #[inline]
    // pub fn remove_relation<R>(
    //     &mut self,
    //     origin: EntityId,
    //     target: EntityId,
    // ) -> Result<R, EntityError>
    // where
    //     R: Relation,
    // {
    //     with_buffer!(self, buffer => {
    //         self.remove_relation_with_buffer::<R>(origin, target, buffer)
    //     })
    // }

    // #[inline]
    // pub(crate) fn remove_relation_with_buffer<R>(
    //     &mut self,
    //     origin: EntityId,
    //     target: EntityId,
    //     buffer: &mut ActionBuffer,
    // ) -> Result<R, EntityError>
    // where
    //     R: Relation,
    // {
    //     self.maintenance();

    //     self.entities.get_location(origin).ok_or(NoSuchEntity)?;
    //     self.entities.get_location(target).ok_or(NoSuchEntity)?;

    //     unsafe {
    //         if let Ok(c) = self.query_one_unchecked::<&mut OriginComponent<R>>(origin) {
    //             if let Some(r) =
    //                 c.remove_relation(origin, target, ActionEncoder::new(buffer, &self.entities))
    //             {
    //                 return Ok(r);
    //             }
    //         }
    //     }
    //     Err(EntityError::MissingComponents)
    // }
}
