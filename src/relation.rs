//! [`Relation`] is a concept that is similar to [`Component`].
//! The main difference is that they are not components, but rather relations.
//!
//! [`Component`] is data that can be attached to individual entity
//! and [`Relation`] is data that connects two entities together.
//!
//! [`Component`]: ../component/trait.Component.html

use core::{marker::PhantomData, mem::ManuallyDrop};

use crate::{action::ActionEncoder, component::Component, entity::EntityId};

/// Trait that must be implemented for relations.
pub trait Relation: Copy + Send + Sync + 'static {
    /// If `true` then relation can be added only once to an entity.
    const EXCLUSIVE: bool = false;

    /// If `true` then when relation is added to an entity
    /// it is also added to the target.
    const SYMMETRIC: bool = false;

    /// Method that is called when relation is removed from origin entity.
    /// Does nothing by default.
    #[inline]
    fn on_drop(&mut self, entity: EntityId, target: EntityId, encoder: &mut ActionEncoder) {
        drop(entity);
        drop(target);
        drop(encoder);
    }

    /// Method that is called when relation is re-inserted.
    /// Does nothing by default and returns `true`, causing `on_origin_drop` to be called.
    #[inline]
    fn on_set(
        &mut self,
        value: &Self,
        entity: EntityId,
        target: EntityId,
        new_target: EntityId,
        encoder: &mut ActionEncoder,
    ) -> bool {
        drop(value);
        drop(entity);
        drop(target);
        drop(new_target);
        drop(encoder);

        true
    }

    /// Method that is called when target entity of the relation is dropped.
    /// Does nothing by default.
    #[inline]
    fn on_target_drop(entity: EntityId, target: EntityId, encoder: &mut ActionEncoder) {
        drop(entity);
        drop(target);
        drop(encoder);
    }
}

pub(crate) struct Origin<R> {
    pub target: EntityId,
    pub relation: R,
}

pub(crate) union OriginComponent<R: Relation> {
    exclusive: ManuallyDrop<Origin<R>>,
    non_exclusive: ManuallyDrop<Vec<Origin<R>>>,
}

impl<R> Drop for OriginComponent<R>
where
    R: Relation,
{
    fn drop(&mut self) {
        match R::EXCLUSIVE {
            false => unsafe { ManuallyDrop::drop(&mut self.non_exclusive) },
            true => unsafe { ManuallyDrop::drop(&mut self.exclusive) },
        }
    }
}

impl<R> OriginComponent<R>
where
    R: Relation,
{
    pub(crate) fn new(target: EntityId, relation: R) -> Self {
        match R::EXCLUSIVE {
            false => OriginComponent {
                non_exclusive: ManuallyDrop::new(vec![Origin { target, relation }]),
            },
            true => OriginComponent {
                exclusive: ManuallyDrop::new(Origin { target, relation }),
            },
        }
    }

    pub(crate) fn set(
        &mut self,
        entity: EntityId,
        target: EntityId,
        relation: R,
        encoder: &mut ActionEncoder,
    ) {
        match R::EXCLUSIVE {
            false => {
                let origins = unsafe { &mut *self.non_exclusive };
                for idx in 0..origins.len() {
                    if origins[idx].target == target {
                        Self::set_one(
                            &mut origins[idx],
                            Origin { target, relation },
                            entity,
                            encoder,
                        );
                        return;
                    }
                }
                origins.push(Origin { target, relation });
            }
            true => {
                let old_origin = unsafe { &mut *self.exclusive };
                Self::set_one(old_origin, Origin { target, relation }, entity, encoder);
            }
        }
    }

    pub fn origins(&self) -> &[Origin<R>] {
        match R::EXCLUSIVE {
            false => unsafe { &*self.non_exclusive },
            true => core::slice::from_ref(unsafe { &*self.exclusive }),
        }
    }

    pub fn origins_mut(&mut self) -> &mut [Origin<R>] {
        match R::EXCLUSIVE {
            false => unsafe { &mut *self.non_exclusive },
            true => core::slice::from_mut(unsafe { &mut *self.exclusive }),
        }
    }

    /// Called when relation is removed from symmetric entity.
    /// Or symmetric entity is dropped.
    fn on_target_drop(&mut self, entity: EntityId, target: EntityId, encoder: &mut ActionEncoder) {
        debug_assert!(!R::EXCLUSIVE);

        let origins = unsafe { &mut *self.non_exclusive };

        for idx in 0..origins.len() {
            if origins[idx].target == target {
                if R::SYMMETRIC {
                    R::on_target_drop(target, entity, encoder)
                };
                origins[idx].relation.on_drop(entity, target, encoder);
                origins.swap_remove(idx);
                break;
            }
        }

        if origins.is_empty() {
            encoder.remove_component::<Self>(target);
        }
    }

    fn drop_one(origin: &mut Origin<R>, entity: EntityId, encoder: &mut ActionEncoder) {
        origin.relation.on_drop(entity, origin.target, encoder);
        if R::SYMMETRIC {
            // This is also a target.
            R::on_target_drop(origin.target, entity, encoder);
        }
        Self::clear_one(origin, entity, encoder);
    }

    fn set_one(
        origin: &mut Origin<R>,
        new_origin: Origin<R>,
        entity: EntityId,
        encoder: &mut ActionEncoder,
    ) {
        let on_set = origin.relation.on_set(
            &new_origin.relation,
            entity,
            origin.target,
            new_origin.target,
            encoder,
        );
        if on_set {
            origin.relation.on_drop(entity, origin.target, encoder);
        }
        if R::SYMMETRIC {
            // This is also a target.
            R::on_target_drop(origin.target, entity, encoder);
        }
        Self::clear_one(origin, entity, encoder);
        origin.relation = new_origin.relation;
    }

    fn clear_one(origin: &mut Origin<R>, entity: EntityId, encoder: &mut ActionEncoder) {
        if R::SYMMETRIC {
            if R::EXCLUSIVE {
                encoder.remove_component::<Self>(origin.target);
            } else {
                let target = origin.target;
                encoder.custom(move |world, encoder| {
                    if let Ok(target_component) = world.query_one_mut::<&mut Self>(&target) {
                        target_component.on_target_drop(target, entity, encoder);
                    }
                });
            }
        } else {
            let target = origin.target;
            encoder.custom(move |world, encoder| {
                if let Ok(target_component) =
                    world.query_one_mut::<&mut TargetComponent<R>>(&target)
                {
                    target_component.on_origin_drop(entity, target, encoder);
                }
            });
        }
    }
}

impl<R> Component for OriginComponent<R>
where
    R: Relation,
{
    #[inline]
    fn on_drop(&mut self, entity: EntityId, encoder: &mut ActionEncoder) {
        for origin in self.origins_mut() {
            Self::drop_one(origin, entity, encoder);
        }
    }

    #[inline]
    fn on_set(&mut self, _value: &Self, _entity: EntityId, _encoder: &mut ActionEncoder) -> bool {
        unimplemented!("This method is not intended to be called");
    }
}

/// Component that is added to target entity of the non-symmetric relation.
pub(crate) struct TargetComponent<R> {
    origins: Vec<EntityId>,
    relation: PhantomData<R>,
}

impl<R> TargetComponent<R>
where
    R: Relation,
{
    pub(crate) fn new(entity: EntityId) -> Self {
        debug_assert!(!R::SYMMETRIC);

        TargetComponent {
            origins: vec![entity],
            relation: PhantomData,
        }
    }

    pub(crate) fn set(&mut self, entity: EntityId) {
        debug_assert!(!self.origins.contains(&entity));
        self.origins.push(entity);
    }

    /// Called when relation is removed from origin entity.
    /// Or origin entity is dropped.
    fn on_origin_drop(&mut self, entity: EntityId, target: EntityId, encoder: &mut ActionEncoder) {
        for idx in 0..self.origins.len() {
            if self.origins[idx] == entity {
                R::on_target_drop(entity, target, encoder);
                self.origins.swap_remove(idx);
                break;
            }
        }

        if self.origins.is_empty() {
            encoder.remove_component::<Self>(target);
        }
    }
}

impl<R> Component for TargetComponent<R>
where
    R: Relation,
{
    #[inline]
    fn on_drop(&mut self, target: EntityId, encoder: &mut ActionEncoder) {
        for &entity in &self.origins {
            R::on_target_drop(entity, target, encoder);
            if R::EXCLUSIVE {
                encoder.remove_component::<OriginComponent<R>>(entity);
            } else {
                encoder.custom(move |world, encoder| {
                    if let Ok(origin) = world.query_one_mut::<&mut OriginComponent<R>>(&entity) {
                        origin.on_target_drop(entity, target, encoder);
                    }
                });
            }
        }
    }

    #[inline]
    fn on_set(&mut self, _value: &Self, _entity: EntityId, _encoder: &mut ActionEncoder) -> bool {
        true
    }
}
