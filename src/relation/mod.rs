//! [`Relation`] is a concept that is similar to [`Component`].
//! The main difference is that they are not components, but rather relations.
//!
//! [`Component`] is data that can be attached to individual entity
//! and [`Relation`] is data that connects two entities together.
//!
//! [`Component`]: ../component/trait.Component.html

use core::{marker::PhantomData, mem::ManuallyDrop};

use alloc::{vec, vec::Vec};

use crate::{
    action::ActionEncoder,
    borrow_dyn_trait,
    component::{Component, ComponentBorrow},
    entity::EntityId,
};

pub use edict_proc::Relation;

pub use self::{
    child_of::ChildOf,
    query::{
        related, related_by, relates, relates_to, FetchFilterRelatedBy, FetchRelated,
        FetchRelatesExclusiveRead, FetchRelatesExclusiveWrite, FetchRelatesRead,
        FetchRelatesToRead, FetchRelatesToWrite, FetchRelatesWrite, FilterFetchRelationTo,
        FilterRelated, FilterRelatedBy, FilterRelates, FilterRelatesTo, Related, Relates,
        RelatesExclusive, RelatesReadIter, RelatesTo, RelatesWriteIter,
    },
};

mod child_of;
mod query;

/// Trait that must be implemented for relations.
pub trait Relation: Send + Sync + Copy + 'static {
    /// If `true` then relation can be added only once to an entity.
    const EXCLUSIVE: bool = false;

    /// If `true` then when relation is added to an entity
    /// it is also added to the target.
    const SYMMETRIC: bool = false;

    /// If `true` then entity in relation is "owned" by the target.
    /// This means that when last target is dropped, entity is also dropped, not just relation.
    const OWNED: bool = false;

    /// Returns name of the relation type.
    #[inline]
    #[must_use]
    fn name() -> &'static str {
        core::any::type_name::<Self>()
    }

    // /// If `true` then when relation is added to an entity,
    // /// the same relation is checked om target and if present,
    // /// target's targets are added as well.
    // /// When target is removed, transitively added targets are removed.
    // const TRANSITIVE: bool = false;

    /// Method that is called when relation is removed from origin entity.
    /// Does nothing by default.
    #[inline]
    fn on_drop(&mut self, entity: EntityId, target: EntityId, encoder: ActionEncoder) {
        drop(entity);
        drop(target);
        drop(encoder);
    }

    /// Method that is called when relation is re-inserted.
    /// Does nothing by default and returns `true`, causing `on_origin_drop` to be called.
    #[inline]
    fn on_replace(
        &mut self,
        value: &Self,
        entity: EntityId,
        target: EntityId,
        new_target: EntityId,
        encoder: ActionEncoder,
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
    fn on_target_drop(entity: EntityId, target: EntityId, encoder: ActionEncoder) {
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
    #[must_use]
    pub fn new(target: EntityId, relation: R) -> Self {
        match R::EXCLUSIVE {
            false => OriginComponent {
                non_exclusive: ManuallyDrop::new(vec![Origin { target, relation }]),
            },
            true => OriginComponent {
                exclusive: ManuallyDrop::new(Origin { target, relation }),
            },
        }
    }

    pub fn add(&mut self, entity: EntityId, target: EntityId, relation: R, encoder: ActionEncoder) {
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

    pub fn remove_relation(
        &mut self,
        entity: EntityId,
        target: EntityId,
        mut encoder: ActionEncoder,
    ) -> Option<R> {
        match R::EXCLUSIVE {
            false => {
                let origins = unsafe { &mut *self.non_exclusive };
                for idx in 0..origins.len() {
                    if origins[idx].target == target {
                        let origin = origins.swap_remove(idx);
                        if origins.is_empty() {
                            encoder.drop::<Self>(entity);
                        }
                        return Some(origin.relation);
                    }
                }
                None
            }
            true => {
                let origin = unsafe { &mut *self.exclusive };
                if origin.target == target {
                    encoder.drop::<Self>(entity);
                    return Some(origin.relation);
                }
                None
            }
        }
    }

    #[must_use]
    pub fn origins(&self) -> &[Origin<R>] {
        match R::EXCLUSIVE {
            false => unsafe { &*self.non_exclusive },
            true => core::slice::from_ref(unsafe { &*self.exclusive }),
        }
    }

    #[must_use]
    pub fn origins_mut(&mut self) -> &mut [Origin<R>] {
        match R::EXCLUSIVE {
            false => unsafe { &mut *self.non_exclusive },
            true => core::slice::from_mut(unsafe { &mut *self.exclusive }),
        }
    }

    /// Called when target relation component is removed from target entity for non-exclusive relations.
    fn on_non_exclusive_target_drop(
        &mut self,
        entity: EntityId,
        target: EntityId,
        mut encoder: ActionEncoder,
    ) {
        debug_assert!(!R::EXCLUSIVE);

        let origins = unsafe { &mut *self.non_exclusive };

        for idx in 0..origins.len() {
            if origins[idx].target == target {
                if R::SYMMETRIC {
                    R::on_target_drop(target, entity, encoder.reborrow())
                };
                origins[idx]
                    .relation
                    .on_drop(entity, target, encoder.reborrow());
                origins.swap_remove(idx);
                break;
            }
        }

        if origins.is_empty() {
            if R::OWNED {
                encoder.despawn(entity);
            } else {
                encoder.drop::<Self>(entity);
            }
        }
    }

    fn drop_one(origin: &mut Origin<R>, entity: EntityId, mut encoder: ActionEncoder) {
        origin
            .relation
            .on_drop(entity, origin.target, encoder.reborrow());
        if R::SYMMETRIC {
            // This is also a target.
            R::on_target_drop(origin.target, entity, encoder.reborrow());
        }
        Self::clear_one(origin, entity, encoder);
    }

    fn set_one(
        origin: &mut Origin<R>,
        new_origin: Origin<R>,
        entity: EntityId,
        mut encoder: ActionEncoder,
    ) {
        let on_replace = origin.relation.on_replace(
            &new_origin.relation,
            entity,
            origin.target,
            new_origin.target,
            encoder.reborrow(),
        );
        if on_replace {
            origin
                .relation
                .on_drop(entity, origin.target, encoder.reborrow());
        }
        if new_origin.target != origin.target {
            Self::clear_one(origin, entity, encoder);
        }
        *origin = new_origin;
    }

    fn clear_one(origin: &mut Origin<R>, entity: EntityId, mut encoder: ActionEncoder) {
        if R::SYMMETRIC {
            if origin.target != entity {
                R::on_target_drop(origin.target, entity, encoder.reborrow());
                if R::EXCLUSIVE {
                    if R::OWNED {
                        encoder.despawn(origin.target);
                    } else {
                        encoder.drop::<Self>(origin.target);
                    }
                } else {
                    let target = origin.target;
                    encoder.closure_with_encoder(move |world, encoder| {
                        if let Ok(mut target_component) = world.query_one::<&mut Self>(target) {
                            if let Some(target_component) = target_component.get() {
                                target_component
                                    .on_non_exclusive_target_drop(target, entity, encoder);
                            }
                        }
                    });
                }
            }
        } else {
            let target = origin.target;
            encoder.closure_with_encoder(move |world, encoder| {
                if let Ok(mut target_component) = world.query_one::<&mut TargetComponent<R>>(target)
                {
                    if let Some(target_component) = target_component.get() {
                        target_component.on_origin_drop(entity, target, encoder);
                    }
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
    fn on_drop(&mut self, entity: EntityId, mut encoder: ActionEncoder) {
        for origin in self.origins_mut() {
            Self::drop_one(origin, entity, encoder.reborrow());
        }
    }

    #[inline]
    fn on_replace(&mut self, _value: &Self, _entity: EntityId, _encoder: ActionEncoder) -> bool {
        unimplemented!("This method is not intended to be called");
    }

    #[inline]
    #[must_use]
    fn borrows() -> Vec<ComponentBorrow> {
        let mut output = Vec::new();
        if R::SYMMETRIC {
            borrow_dyn_trait!(Self as RelationOrigin => output);
            borrow_dyn_trait!(Self as RelationTarget => output);
        } else {
            borrow_dyn_trait!(Self as RelationOrigin => output);
        }
        output
    }
}

/// Component that is added to target entity of the non-symmetric relation.
pub(crate) struct TargetComponent<R> {
    origins: Vec<EntityId>,
    relation: PhantomData<fn() -> R>,
}

impl<R> TargetComponent<R>
where
    R: Relation,
{
    #[must_use]
    pub(crate) fn new(entity: EntityId) -> Self {
        debug_assert!(!R::SYMMETRIC);

        TargetComponent {
            origins: vec![entity],
            relation: PhantomData,
        }
    }

    pub(crate) fn add(&mut self, entity: EntityId) {
        debug_assert!(!self.origins.contains(&entity));
        self.origins.push(entity);
    }

    /// Called when relation is removed from origin entity.
    /// Or origin entity is dropped.
    fn on_origin_drop(&mut self, entity: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        for idx in 0..self.origins.len() {
            if self.origins[idx] == entity {
                R::on_target_drop(entity, target, encoder.reborrow());
                self.origins.swap_remove(idx);
                break;
            }
        }

        if self.origins.is_empty() {
            encoder.drop::<Self>(target);
        }
    }
}

impl<R> Component for TargetComponent<R>
where
    R: Relation,
{
    #[inline]
    fn on_drop(&mut self, target: EntityId, mut encoder: ActionEncoder) {
        for &entity in &self.origins {
            R::on_target_drop(entity, target, encoder.reborrow());
            if R::EXCLUSIVE {
                if R::OWNED {
                    encoder.despawn(entity);
                } else {
                    encoder.drop::<OriginComponent<R>>(entity);
                }
            } else {
                encoder.closure_with_encoder(move |world, encoder| unsafe {
                    if let Ok(origin) = world.query_one_unchecked::<&mut OriginComponent<R>>(entity)
                    {
                        origin.on_non_exclusive_target_drop(entity, target, encoder);
                    }
                });
            }
        }
    }

    #[inline]
    fn on_replace(&mut self, _value: &Self, _entity: EntityId, _encoder: ActionEncoder) -> bool {
        unimplemented!("This method is not intended to be called");
    }

    #[inline]
    #[must_use]
    fn borrows() -> Vec<ComponentBorrow> {
        let mut output = Vec::new();
        borrow_dyn_trait!(Self as RelationTarget => output);
        output
    }
}

#[doc(hidden)]
pub trait RelationOrigin {
    fn targets(&self) -> Vec<EntityId>;
}

impl<R> RelationOrigin for OriginComponent<R>
where
    R: Relation,
{
    #[must_use]
    fn targets(&self) -> Vec<EntityId> {
        self.origins().iter().map(|o| o.target).collect()
    }
}

#[doc(hidden)]
pub trait RelationTarget {
    #[must_use]
    fn origins(&self) -> Vec<EntityId>;
}

impl<R> RelationTarget for OriginComponent<R>
where
    R: Relation,
{
    fn origins(&self) -> Vec<EntityId> {
        debug_assert!(R::SYMMETRIC);

        self.origins().iter().map(|o| o.target).collect()
    }
}

impl<R> RelationTarget for TargetComponent<R>
where
    R: Relation,
{
    fn origins(&self) -> Vec<EntityId> {
        debug_assert!(!R::SYMMETRIC);

        self.origins.clone()
    }
}
