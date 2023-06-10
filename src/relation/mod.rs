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

    /// Method that is called when relation is removed from origin entity.
    /// Does nothing by default.
    #[inline]
    fn on_drop(&mut self, id: EntityId, target: EntityId, encoder: ActionEncoder) {
        drop(id);
        drop(target);
        drop(encoder);
    }

    /// Method that is called when relation is re-inserted.
    /// Does nothing by default and returns `true`, causing `on_origin_drop` to be called.
    #[inline]
    fn on_replace(
        &mut self,
        value: &Self,
        id: EntityId,
        target: EntityId,
        new_target: EntityId,
        encoder: ActionEncoder,
    ) -> bool {
        drop(value);
        drop(id);
        drop(target);
        drop(new_target);
        drop(encoder);

        true
    }

    /// Method that is called when target entity of the relation is dropped.
    /// Does nothing by default.
    #[inline]
    fn on_target_drop(id: EntityId, target: EntityId, encoder: ActionEncoder) {
        drop(id);
        drop(target);
        drop(encoder);
    }
}

pub(crate) struct RelationTarget<R> {
    pub target: EntityId,
    pub relation: R,
}

pub(crate) union OriginComponent<R: Relation> {
    exclusive: ManuallyDrop<RelationTarget<R>>,
    non_exclusive: ManuallyDrop<Vec<RelationTarget<R>>>,
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
    /// Called when new relation is added to an entity.
    #[must_use]
    pub fn new_relation(target: EntityId, relation: R) -> Self {
        match R::EXCLUSIVE {
            false => OriginComponent {
                non_exclusive: ManuallyDrop::new(vec![RelationTarget { target, relation }]),
            },
            true => OriginComponent {
                exclusive: ManuallyDrop::new(RelationTarget { target, relation }),
            },
        }
    }

    /// Called when new relation is added to an entity that already has relation of this type.
    pub fn add_relation(
        &mut self,
        id: EntityId,
        target: EntityId,
        relation: R,
        encoder: ActionEncoder,
    ) {
        match R::EXCLUSIVE {
            false => {
                let relations = unsafe { &mut *self.non_exclusive };
                for idx in 0..relations.len() {
                    if relations[idx].target == target {
                        Self::set_one(
                            &mut relations[idx],
                            RelationTarget { target, relation },
                            id,
                            encoder,
                        );
                        return;
                    }
                }
                relations.push(RelationTarget { target, relation });
            }
            true => {
                let old_origin = unsafe { &mut *self.exclusive };
                Self::set_one(old_origin, RelationTarget { target, relation }, id, encoder);
            }
        }
    }

    /// Called when relation is removed from an entity.
    /// This won't trigger any hooks.
    pub fn remove_relation(
        &mut self,
        id: EntityId,
        target: EntityId,
        mut encoder: ActionEncoder,
    ) -> Option<R> {
        match R::EXCLUSIVE {
            false => {
                let relations = unsafe { &mut *self.non_exclusive };
                for idx in 0..relations.len() {
                    if relations[idx].target == target {
                        let r = relations.swap_remove(idx);
                        if relations.is_empty() {
                            encoder.drop::<Self>(id);
                        }
                        return Some(r.relation);
                    }
                }
                None
            }
            true => {
                let r = unsafe { &mut *self.exclusive };
                if r.target == target {
                    encoder.drop::<Self>(id);
                    return Some(r.relation);
                }
                None
            }
        }
    }

    /// Called by target relation component when it is dropped or replaced.
    fn on_target_drop(origin: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        if R::EXCLUSIVE {
            encoder.drop::<Self>(origin);
        } else {
            encoder.closure(|world, mut encoder| {
                let Ok(origin) = world.lookup(origin) else { return; };
                let Some(comp) = world.get::<&mut Self>(origin) else { return; };

                let origins = unsafe { &mut *comp.non_exclusive };

                for idx in 0..origins.len() {
                    if origins[idx].target == target {
                        origins[idx]
                            .relation
                            .on_drop(origin, target, encoder.reborrow());
                        origins.swap_remove(idx);
                        break;
                    }
                }

                if origins.is_empty() {
                    if R::OWNED {
                        encoder.despawn(comp);
                    } else {
                        encoder.drop::<Self>(comp);
                    }
                }
            });
        }
    }

    #[must_use]
    pub fn origins(&self) -> &[RelationTarget<R>] {
        match R::EXCLUSIVE {
            false => unsafe { &*self.non_exclusive },
            true => core::slice::from_ref(unsafe { &*self.exclusive }),
        }
    }

    #[must_use]
    pub fn origins_mut(&mut self) -> &mut [RelationTarget<R>] {
        match R::EXCLUSIVE {
            false => unsafe { &mut *self.non_exclusive },
            true => core::slice::from_mut(unsafe { &mut *self.exclusive }),
        }
    }

    fn drop_one(relation: &mut R, origin: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        relation.on_drop(origin, target, encoder.reborrow());
        if R::SYMMETRIC {
            // This is also a target.
            R::on_target_drop(origin.target, origin, encoder.reborrow());
        }
        Self::clear_one(relation, origin, target, encoder);
    }

    fn set_one(
        relation: &mut R,
        new_relation: R,
        origin: EntityId,
        target: EntityId,
        new_target: EntityId,
        mut encoder: ActionEncoder,
    ) {
        let on_replace = relation.on_replace(
            &new_relation,
            origin,
            target,
            new_target,
            encoder.reborrow(),
        );
        if on_replace {
            relation.on_drop(origin, target, encoder.reborrow());
        }
        if new_target != origin.target {
            Self::clear_one(relation, origin, target, encoder);
        }
        *relation = new_relation;
    }

    fn clear_one(relation: &mut R, origin: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        if R::SYMMETRIC {
            if target != origin {
                R::on_target_drop(target, origin, encoder.reborrow());
                if R::EXCLUSIVE {
                    if R::OWNED {
                        encoder.despawn(target);
                    } else {
                        encoder.drop::<Self>(target);
                    }
                } else {
                    let target = target;
                    encoder.closure_with_encoder(move |world, encoder| {
                        if let Ok(mut target_component) = world.query_one::<&mut Self>(target) {
                            if let Some(target_component) = target_component.get() {
                                target_component
                                    .on_non_exclusive_target_drop(target, origin, encoder);
                            }
                        }
                    });
                }
            }
        } else {
            let target = target;
            encoder.closure_with_encoder(move |world, encoder| {
                if let Ok(mut target_component) = world.query_one::<&mut TargetComponent<R>>(target)
                {
                    if let Some(target_component) = target_component.get() {
                        target_component.on_origin_drop(origin, target, encoder);
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
    fn on_drop(&mut self, id: EntityId, mut encoder: ActionEncoder) {
        for origin in self.origins_mut() {
            Self::drop_one(origin, id, encoder.reborrow());
        }
    }

    #[inline]
    fn on_replace(&mut self, _value: &Self, _id: EntityId, _encoder: ActionEncoder) -> bool {
        unimplemented!("This method is not intended to be called");
    }

    #[inline]
    #[must_use]
    fn borrows() -> Vec<ComponentBorrow> {
        Vec::new()
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
    pub(crate) fn new(id: EntityId) -> Self {
        debug_assert!(!R::SYMMETRIC);

        TargetComponent {
            origins: vec![id],
            relation: PhantomData,
        }
    }

    pub(crate) fn add(&mut self, id: EntityId) {
        debug_assert!(!self.origins.contains(&id));
        self.origins.push(id);
    }

    /// Called when relation is removed from origin entity.
    /// Or origin entity is dropped.
    fn on_origin_drop(&mut self, id: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        for idx in 0..self.origins.len() {
            if self.origins[idx] == id {
                R::on_target_drop(id, target, encoder.reborrow());
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
        Vec::new()
    }
}
