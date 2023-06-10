//! [`Relation`] is a concept that is similar to [`Component`].
//! The main difference is that they are not components, but rather relations.
//!
//! [`Component`] is data that can be attached to individual entity
//! and [`Relation`] is data that connects two entities together.
//!
//! [`Component`]: ../component/trait.Component.html

use core::marker::PhantomData;

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

pub trait RelationExclusivity<R> {
    type OriginComponent: Component;
    type Origins: ?Sized;
    type Relates<'a>;
    type RelatesMut<'a>;

    #[must_use]
    fn new(target: EntityId, relation: R) -> Self::OriginComponent;
    fn add(
        comp: &mut Self::OriginComponent,
        id: EntityId,
        target: EntityId,
        relation: R,
        encoder: ActionEncoder,
    );

    #[must_use]
    fn remove(
        comp: &mut Self::OriginComponent,
        id: EntityId,
        target: EntityId,
        encoder: ActionEncoder,
    ) -> Option<R>;

    #[must_use]
    fn origins(comp: &Self::OriginComponent) -> &Self::Origins;

    #[must_use]
    fn relates(comp: &Self::OriginComponent) -> Self::Relates<'_>;

    #[must_use]
    fn relates_mut(comp: &mut Self::OriginComponent) -> Self::RelatesMut<'_>;

    fn on_target_drop(id: EntityId, target: EntityId, encoder: ActionEncoder);
}

pub trait RelationSymmetry<R> {
    type TargetComponent: Component;

    fn on_origin_drop(relation: &mut R, id: EntityId, target: EntityId, encoder: ActionEncoder);
}

pub struct NonSymmetric;

impl<R> RelationSymmetry<R> for NonSymmetric
where
    R: Relation<Symmetric = NonSymmetric>,
{
    type TargetComponent = TargetComponent<R>;

    #[inline(always)]
    fn on_origin_drop(relation: &mut R, id: EntityId, target: EntityId, encoder: ActionEncoder) {}
}

pub trait RelationOwnership<R> {}

/// Trait that must be implemented for relations.
pub trait Relation: Send + Sync + Copy + 'static {
    type Exclusive: RelationExclusivity<Self>;
    type Symmetric: RelationSymmetry<Self>;
    type Ownership: RelationOwnership<Self>;

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
    #[must_use]
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

pub(crate) struct Origin<R> {
    pub target: EntityId,
    pub relation: R,
}

struct OriginComponent<R> {
    origins: Vec<Origin<R>>,
}

pub struct NonExclusive;

impl<R> RelationExclusivity<R> for NonExclusive
where
    R: Relation<Exclusive = NonExclusive>,
{
    type OriginComponent = OriginComponent<R>;
    type Origins = [Origin<R>];
    type Relates<'a> = RelatesReadIter<'a, R>;
    type RelatesMut<'a> = RelatesWriteIter<'a, R>;

    fn new(target: EntityId, relation: R) -> OriginComponent<R> {
        OriginComponent {
            origins: vec![Origin { target, relation }],
        }
    }

    fn add(
        comp: &mut OriginComponent<R>,
        id: EntityId,
        target: EntityId,
        relation: R,
        encoder: ActionEncoder,
    ) {
        for idx in 0..comp.origins.len() {
            if comp.origins[idx].target == target {
                OriginComponent::set_one(
                    &mut comp.origins[idx],
                    Origin { target, relation },
                    id,
                    encoder,
                );
                return;
            }
        }
        comp.origins.push(Origin { target, relation });
    }

    fn remove(
        comp: &mut OriginComponent<R>,
        id: EntityId,
        target: EntityId,
        mut encoder: ActionEncoder,
    ) -> Option<R> {
        for idx in 0..comp.origins.len() {
            if comp.origins[idx].target == target {
                let origin = comp.origins.swap_remove(idx);
                if comp.origins.is_empty() {
                    encoder.drop::<Self>(id);
                }
                return Some(origin.relation);
            }
        }
        None
    }

    fn origins(comp: &OriginComponent<R>) -> &[Origin<R>] {
        &comp.origins
    }

    fn relates(comp: &Self::OriginComponent) -> Self::Relates<'_> {
        todo!()
    }

    fn relates_mut(comp: &mut Self::OriginComponent) -> Self::RelatesMut<'_> {
        todo!()
    }

    /// Called when target relation component is removed from target entity for non-exclusive relations.
    fn on_target_drop(id: EntityId, target: EntityId, mut encoder: ActionEncoder) {
        encoder.closure(|world| {
            let Ok(comp) = world.query_one::<>(id)
        });

    }
}

impl<R> OriginComponent<R>
where
    R: Relation<Exclusive = NonExclusive>,
{
    fn drop_one(origin: &mut Origin<R>, id: EntityId, mut encoder: ActionEncoder) {
        <R::Symmetric as RelationSymmetry<R>>::on_origin_drop(
            &mut origin.relation,
            origin.target,
            id,
            encoder.reborrow(),
        );

        origin
            .relation
            .on_drop(id, origin.target, encoder.reborrow());

        if R::SYMMETRIC {
            // This is also a target.
            R::on_target_drop(origin.target, id, encoder.reborrow());
        }
        Self::clear_one(origin, id, encoder);
    }

    fn set_one(
        origin: &mut Origin<R>,
        new_origin: Origin<R>,
        id: EntityId,
        mut encoder: ActionEncoder,
    ) {
        let on_replace = origin.relation.on_replace(
            &new_origin.relation,
            id,
            origin.target,
            new_origin.target,
            encoder.reborrow(),
        );
        if on_replace {
            origin
                .relation
                .on_drop(id, origin.target, encoder.reborrow());
        }
        if new_origin.target != origin.target {
            Self::clear_one(origin, id, encoder);
        }
        *origin = new_origin;
    }

    fn clear_one(origin: &mut Origin<R>, id: EntityId, mut encoder: ActionEncoder) {
        if R::SYMMETRIC {
            if origin.target != id {
                R::on_target_drop(origin.target, id, encoder.reborrow());
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
                                target_component.on_non_exclusive_target_drop(target, id, encoder);
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
                        target_component.on_origin_drop(id, target, encoder);
                    }
                }
            });
        }
    }
}

impl<R> Component for OriginComponent<R>
where
    R: Relation<Exclusive = NonExclusive>,
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
