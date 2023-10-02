//! [`Relation`] is a concept that is similar to [`Component`].
//! The main difference is that they are not components, but rather relations.
//!
//! [`Component`] is data that can be attached to individual entity
//! and [`Relation`] is data that connects two entities together.
//!
//! [`Component`]: ../component/trait.Component.html

use alloc::{vec, vec::Vec};
use core::{marker::PhantomData, mem::ManuallyDrop};

use crate::{
    action::ActionEncoder,
    component::{Component, ComponentBorrow},
    entity::EntityId,
};

pub use edict_proc::Relation;

pub use self::{
    child_of::ChildOf,
    query::{
        FetchFilterRelatedBy, FetchRelated, FetchRelatesExclusiveRead, FetchRelatesExclusiveWrite,
        FetchRelatesRead, FetchRelatesToRead, FetchRelatesToWrite, FetchRelatesWrite,
        FilterFetchRelatesTo, FilterRelated, FilterRelatedBy, FilterRelates, FilterRelatesTo,
        Related, Relates, RelatesExclusive, RelatesReadIter, RelatesTo, RelatesWriteIter,
    },
};

mod child_of;
mod query;

/// Trait that must be implemented for types to be
/// used as relation components.
///
/// Relation components are special in a way that they are bound to
/// a pair of entities, not just one.
/// One entity is called "origin" and the other is called "target".
///
/// Relation components are used to connect two entities together.
/// For example [`ChildOf`] relation component is used to connect
/// child entity ("origin") to parent entity ("target").
///
/// Relation components are dropped when either of the "origin" or "target"
/// is dropped. Appropriate hook method is called when this happens.
/// `on_drop` is called when relation is dropped from "origin" entity.
/// `on_target_drop` is called when "target" entity is dropped.
pub trait Relation: Send + Sync + Copy + 'static {
    /// If `true` then relation can be added only once to an entity.
    /// If another exclusive relation is added to the same entity,
    /// then the old one is removed.
    /// `on_replace` is called when this happens.
    ///
    /// Non-exclusive relations is replaced only if re-added
    /// with same target.
    const EXCLUSIVE: bool = false;

    /// If `true` then when relation is added to an entity
    /// it is also added to the target in reverse direction.
    const SYMMETRIC: bool = false;

    /// If `true` then origin entity in relation is "owned" by the target.
    /// This means that when last target is dropped, entity is despawned.
    const OWNED: bool = false;

    /// Returns name of the relation type.
    ///
    /// Can be overriden to provide custom name.
    #[inline(always)]
    #[must_use]
    fn name() -> &'static str {
        core::any::type_name::<Self>()
    }

    /// Method that is called when relation is dropped on origin entity.
    /// Does nothing by default.
    #[inline(always)]
    fn on_drop(&mut self, origin: EntityId, target: EntityId, encoder: ActionEncoder) {
        drop(origin);
        drop(target);
        drop(encoder);
    }

    /// Method that is called when relation is re-inserted.
    /// For non-exclusive relations this happens when relation is re-inserted with the same
    /// origin-target entity pair.
    /// For exclusive relations this happens when relation is re-inserted with
    /// origin that has relation of this type with any target.
    ///
    /// If returns `true`, `on_drop` will be called.
    ///
    /// Does nothing by default and returns `true`, causing `on_drop` to be called.
    #[inline(always)]
    fn on_replace(
        &mut self,
        value: &Self,
        origin: EntityId,
        target: EntityId,
        new_target: EntityId,
        encoder: ActionEncoder,
    ) -> bool {
        drop(value);
        drop(origin);
        drop(target);
        drop(new_target);
        drop(encoder);

        true
    }

    /// Method that is called when target entity of the relation is dropped.
    ///
    /// Does nothing by default.
    #[inline(always)]
    fn on_target_drop(origin: EntityId, target: EntityId, encoder: ActionEncoder) {
        drop(origin);
        drop(target);
        drop(encoder);
    }
}

/// Sub-trait for exclusive relations.
/// It should be implemented for relations that specify `EXCLUSIVE = true`,
/// to enable use of `RelatesExclusive` query.
/// Implementing it for relation with `EXCLUSIVE = false` will cause
/// compilation error or runtime panic.
///
/// `Relation` derive macro implements this trait automatically.
pub trait ExclusiveRelation: Relation {
    #[doc(hidden)]
    const ASSERT_EXCLUSIVE: () = assert!(Self::EXCLUSIVE);
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
        origin: EntityId,
        target: EntityId,
        relation: R,
        encoder: ActionEncoder,
    ) {
        match R::EXCLUSIVE {
            false => {
                let relations = unsafe { &mut *self.non_exclusive };
                for r in relations.iter_mut() {
                    if r.target == target {
                        Self::set_one(&mut r.relation, relation, origin, target, target, encoder);
                        return;
                    }
                }
                relations.push(RelationTarget { target, relation });
            }
            true => {
                let r = unsafe { &mut *self.exclusive };
                Self::set_one(&mut r.relation, relation, origin, r.target, target, encoder);
                r.target = target;
            }
        }
    }

    /// Called when relation is removed from an entity.
    /// This won't trigger any hooks.
    pub fn remove_relation(
        &mut self,
        origin: EntityId,
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
                            encoder.drop::<Self>(origin);
                        }
                        return Some(r.relation);
                    }
                }
                None
            }
            true => {
                let r = unsafe { &mut *self.exclusive };
                if r.target == target {
                    encoder.drop::<Self>(origin);
                    return Some(r.relation);
                }
                None
            }
        }
    }

    /// Called by target relation component when it is dropped or replaced.
    fn on_target_drop(origin: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        if R::EXCLUSIVE {
            if R::OWNED {
                encoder.despawn(origin);
            } else {
                encoder.drop::<Self>(origin);
            }
        } else {
            encoder.closure(move |world| {
                let Ok(mut origin) = world.entity(origin) else {
                    return;
                };

                let Some(comp) = origin.get::<&mut Self>() else {
                    return;
                };

                let origins = unsafe { &mut *comp.non_exclusive };

                for idx in 0..origins.len() {
                    if origins[idx].target == target {
                        origins.swap_remove(idx);
                        break;
                    }
                }

                if origins.is_empty() {
                    if R::OWNED {
                        origin.despawn();
                    } else {
                        origin.drop::<Self>();
                    }
                }
            });
        }
    }

    #[must_use]
    pub fn relations(&self) -> &[RelationTarget<R>] {
        match R::EXCLUSIVE {
            false => unsafe { &*self.non_exclusive },
            true => core::slice::from_ref(unsafe { &*self.exclusive }),
        }
    }

    #[must_use]
    pub fn relations_mut(&mut self) -> &mut [RelationTarget<R>] {
        match R::EXCLUSIVE {
            false => unsafe { &mut *self.non_exclusive },
            true => core::slice::from_mut(unsafe { &mut *self.exclusive }),
        }
    }

    fn drop_one(relation: &mut R, origin: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        relation.on_drop(origin, target, encoder.reborrow());
        if R::SYMMETRIC {
            if target != origin {
                Self::on_target_drop(target, origin, encoder);
            }
        } else {
            TargetComponent::<R>::on_origin_drop(origin, target, encoder)
        }
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
        if new_target != target {
            if R::SYMMETRIC {
                if target != origin {
                    Self::on_target_drop(target, origin, encoder);
                }
            } else {
                TargetComponent::<R>::on_origin_drop(origin, target, encoder)
            }
        }
        *relation = new_relation;
    }
}

impl<R> Component for OriginComponent<R>
where
    R: Relation,
{
    #[inline(always)]
    fn on_drop(&mut self, origin: EntityId, mut encoder: ActionEncoder) {
        for r in self.relations_mut() {
            Self::drop_one(&mut r.relation, origin, r.target, encoder.reborrow());
        }
    }

    #[inline(always)]
    fn on_replace(&mut self, _value: &Self, _origin: EntityId, _encoder: ActionEncoder) -> bool {
        unimplemented!("This method is not intended to be called");
    }

    #[inline(always)]
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
    pub(crate) fn new(origin: EntityId) -> Self {
        debug_assert!(!R::SYMMETRIC);

        TargetComponent {
            origins: vec![origin],
            relation: PhantomData,
        }
    }

    pub(crate) fn add(&mut self, origin: EntityId) {
        debug_assert!(!self.origins.contains(&origin));
        self.origins.push(origin);
    }

    /// Called when relation is removed from an entity.
    /// This won't trigger any hooks.
    pub fn remove_relation(
        &mut self,
        origin: EntityId,
        target: EntityId,
        mut encoder: ActionEncoder,
    ) {
        for idx in 0..self.origins.len() {
            if self.origins[idx] == origin {
                self.origins.swap_remove(idx);
                if self.origins.is_empty() {
                    encoder.drop::<Self>(target);
                }
            }
        }
    }

    /// Called when relation is removed from origin entity.
    /// Or origin entity is dropped.
    fn on_origin_drop(origin: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        encoder.closure(move |world| {
            let Ok(mut target) = world.entity(target) else {
                return;
            };
            let Some(comp) = target.get::<&mut Self>() else {
                return;
            };

            for idx in 0..comp.origins.len() {
                if comp.origins[idx] == origin {
                    comp.origins.swap_remove(idx);
                    break;
                }
            }

            if comp.origins.is_empty() {
                target.drop::<Self>();
            }
        })
    }
}

impl<R> Component for TargetComponent<R>
where
    R: Relation,
{
    #[inline(always)]
    fn on_drop(&mut self, target: EntityId, mut encoder: ActionEncoder) {
        for &origin in &self.origins {
            R::on_target_drop(origin, target, encoder.reborrow());
            OriginComponent::<R>::on_target_drop(origin, target, encoder.reborrow());
        }
    }

    #[inline(always)]
    fn on_replace(&mut self, _value: &Self, _entity: EntityId, _encoder: ActionEncoder) -> bool {
        unimplemented!("This method is not intended to be called");
    }

    #[inline(always)]
    #[must_use]
    fn borrows() -> Vec<ComponentBorrow> {
        Vec::new()
    }
}
