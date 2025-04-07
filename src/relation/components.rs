use alloc::{vec, vec::Vec};
use core::{marker::PhantomData, mem::ManuallyDrop};
use smallvec::SmallVec;

use crate::{
    action::LocalActionEncoder,
    component::{Component, ComponentBorrow},
    entity::EntityId,
};

use super::Relation;

pub(crate) union OriginComponent<R: Relation> {
    exclusive: ManuallyDrop<(EntityId, R)>,
    non_exclusive: ManuallyDrop<Vec<(EntityId, R)>>,
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
                non_exclusive: ManuallyDrop::new(vec![(target, relation)]),
            },
            true => OriginComponent {
                exclusive: ManuallyDrop::new((target, relation)),
            },
        }
    }

    /// Called when new relation is added to an entity that already has relation of this type.
    pub fn insert_relation(
        &mut self,
        origin: EntityId,
        target: EntityId,
        relation: R,
        mut encoder: LocalActionEncoder,
    ) -> bool {
        match R::EXCLUSIVE {
            false => {
                let relations = unsafe { &mut *self.non_exclusive };
                for r in relations.iter_mut() {
                    if r.0 == target {
                        let call_on_drop = R::on_replace(
                            &mut r.1,
                            &relation,
                            origin,
                            target,
                            target,
                            encoder.reborrow(),
                        );
                        if call_on_drop {
                            R::on_drop(r.1, origin, r.0, encoder.reborrow());
                        }

                        r.1 = relation;
                        return false;
                    }
                }
                relations.push((target, relation));
                return true;
            }
            true => {
                let r = unsafe { &mut *self.exclusive };

                let call_on_drop =
                    R::on_replace(&mut r.1, &relation, origin, r.0, target, encoder.reborrow());
                if call_on_drop {
                    R::on_drop(r.1, origin, r.0, encoder.reborrow());
                }

                if r.0 != target {
                    if R::SYMMETRIC {
                        if r.0 != origin {
                            Self::on_target_drop(core::iter::once(r.0), origin, encoder);
                        }
                    } else {
                        TargetComponent::<R>::on_origin_drop(
                            origin,
                            core::iter::once(r.0),
                            encoder,
                        );
                    }
                    r.0 = target;
                    return true;
                } else {
                    return false;
                }
            }
        }
    }

    /// Called when relation is removed from an entity.
    /// This won't trigger any hooks.
    pub fn remove_relation(
        &mut self,
        origin: EntityId,
        target: EntityId,
        mut encoder: LocalActionEncoder,
    ) -> Option<R> {
        match R::EXCLUSIVE {
            false => {
                let relations = unsafe { &mut *self.non_exclusive };
                for idx in 0..relations.len() {
                    if relations[idx].0 == target {
                        let r = relations.swap_remove(idx);
                        if relations.is_empty() {
                            encoder.drop::<Self>(origin);
                        }
                        return Some(r.1);
                    }
                }
                None
            }
            true => {
                let r = unsafe { &mut *self.exclusive };
                if r.0 == target {
                    encoder.drop::<Self>(origin);
                    return Some(r.1);
                }
                None
            }
        }
    }

    /// Called by target relation component when it is dropped or replaced.
    fn on_target_drop(
        origins: impl Iterator<Item = EntityId>,
        target: EntityId,
        mut encoder: LocalActionEncoder,
    ) {
        if R::EXCLUSIVE {
            if R::OWNED {
                encoder.despawn_batch(origins);
            } else {
                encoder.drop_batch::<Self>(origins);
            }
        } else {
            let origins = origins.collect::<SmallVec<[_; 8]>>();

            encoder.closure(move |world| {
                for origin in origins {
                    let Ok(mut origin) = world.entity(origin) else {
                        continue;
                    };

                    let Some(comp) = origin.get_mut::<&mut Self>() else {
                        continue;
                    };

                    let targets = unsafe { &mut *comp.non_exclusive };

                    for idx in 0..targets.len() {
                        if targets[idx].0 == target {
                            targets.swap_remove(idx);
                            break;
                        }
                    }

                    if targets.is_empty() {
                        if R::OWNED {
                            origin.despawn();
                        } else {
                            origin.drop::<Self>();
                        }
                    }
                }
            });
        }
    }

    #[must_use]
    pub fn targets(&self) -> &[(EntityId, R)] {
        match R::EXCLUSIVE {
            false => unsafe { &*self.non_exclusive },
            true => core::slice::from_ref(unsafe { &*self.exclusive }),
        }
    }

    #[must_use]
    pub fn targets_mut(&mut self) -> &mut [(EntityId, R)] {
        match R::EXCLUSIVE {
            false => unsafe { &mut *self.non_exclusive },
            true => core::slice::from_mut(unsafe { &mut *self.exclusive }),
        }
    }
}

impl<R> Component for OriginComponent<R>
where
    R: Relation,
{
    #[inline(always)]
    fn on_drop(&mut self, origin: EntityId, mut encoder: LocalActionEncoder) {
        if !self.targets().is_empty() {
            R::on_origin_drop(origin, self.targets(), encoder.reborrow());

            if R::SYMMETRIC {
                Self::on_target_drop(
                    self.targets().iter().map(|r| r.0),
                    origin,
                    encoder.reborrow(),
                );
            } else {
                TargetComponent::<R>::on_origin_drop(
                    origin,
                    self.targets().iter().map(|r| r.0),
                    encoder.reborrow(),
                );
            }
        }
    }

    #[inline(always)]
    fn on_replace(
        &mut self,
        _value: &Self,
        _origin: EntityId,
        _encoder: LocalActionEncoder,
    ) -> bool {
        unimplemented!("This method is not intended to be called");
    }

    #[inline(always)]
    fn borrows() -> Vec<ComponentBorrow> {
        Vec::new()
    }
}

/// Component that is added to target entity of the non-symmetric relation.
#[repr(transparent)]
pub(crate) struct TargetComponent<R> {
    origins: Vec<(EntityId, R)>,
    relation: PhantomData<fn() -> R>,
}

impl<R> TargetComponent<R>
where
    R: Relation,
{
    #[must_use]
    pub fn new(origin: EntityId, relation: R) -> Self {
        debug_assert!(!R::SYMMETRIC);

        TargetComponent {
            origins: vec![(origin, relation)],
            relation: PhantomData,
        }
    }

    pub fn add(&mut self, origin: EntityId, relation: R) {
        debug_assert!(!R::SYMMETRIC);
        debug_assert!(self.origins.iter().all(|r| r.0 != origin));
        self.origins.push((origin, relation));
    }

    /// Called when relation is removed from an entity.
    /// This won't trigger any hooks.
    pub fn remove_relation(
        &mut self,
        origin: EntityId,
        target: EntityId,
        mut encoder: LocalActionEncoder,
    ) {
        debug_assert!(!R::SYMMETRIC);
        for idx in 0..self.origins.len() {
            if self.origins[idx].0 == origin {
                self.origins.swap_remove(idx);
                if self.origins.is_empty() {
                    encoder.drop::<Self>(target);
                }
                return;
            }
        }
    }

    /// Called when relation is removed from origin entity.
    /// Or origin entity is dropped.
    fn on_origin_drop(
        origin: EntityId,
        targets: impl Iterator<Item = EntityId>,
        mut encoder: LocalActionEncoder,
    ) {
        debug_assert!(!R::SYMMETRIC);
        let targets = targets.collect::<SmallVec<[_; 8]>>();

        encoder.closure(move |world| {
            for target in targets {
                let Ok(mut target) = world.entity(target) else {
                    return;
                };
                let Some(comp) = target.get_mut::<&mut Self>() else {
                    return;
                };

                for idx in 0..comp.origins.len() {
                    if comp.origins[idx].0 == origin {
                        comp.origins.swap_remove(idx);
                        break;
                    }
                }

                if comp.origins.is_empty() {
                    target.drop::<Self>();
                }
            }
        })
    }

    pub fn origins(&self) -> &[(EntityId, R)] {
        debug_assert!(!R::SYMMETRIC);
        &self.origins
    }

    pub fn origins_mut(&mut self) -> &mut [(EntityId, R)] {
        debug_assert!(!R::SYMMETRIC);
        &mut self.origins
    }
}

impl<R> Component for TargetComponent<R>
where
    R: Relation,
{
    #[inline(always)]
    fn on_drop(&mut self, target: EntityId, mut encoder: LocalActionEncoder) {
        debug_assert!(!R::SYMMETRIC);

        if !self.origins.is_empty() {
            R::on_target_drop(&self.origins, target, encoder.reborrow());

            OriginComponent::<R>::on_target_drop(
                self.origins.iter().map(|r| r.0),
                target,
                encoder.reborrow(),
            );
        }
    }

    #[inline(always)]
    fn on_replace(
        &mut self,
        _value: &Self,
        _entity: EntityId,
        _encoder: LocalActionEncoder,
    ) -> bool {
        debug_assert!(!R::SYMMETRIC);
        unimplemented!("This method is not intended to be called");
    }

    #[inline(always)]
    fn borrows() -> Vec<ComponentBorrow> {
        debug_assert!(!R::SYMMETRIC);
        Vec::new()
    }
}
