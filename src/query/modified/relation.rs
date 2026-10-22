use core::{any::TypeId, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        Access, AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, Read, SendQuery,
        With, Write, WriteAlias,
    },
    relation::{
        ExclusiveRelation, FilterRelated, FilterRelatedBy, FilterRelates, FilterRelatesTo,
        OriginComponent, Related, Relates, RelatesExclusive, RelatesTo, Relation, TargetComponent,
    },
    system::QueryArg,
    type_id,
    world::World,
};

use super::Modified;

/// Fetch for modification-filtered relation queries.
pub struct ModifiedRelationFetch<F> {
    fetch: F,
    after_epoch: EpochId,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
}

unsafe impl<'a, F: Fetch<'a>> Fetch<'a> for ModifiedRelationFetch<F> {
    type Item = F::Item;

    #[inline]
    fn dangling() -> Self {
        Self {
            fetch: F::dangling(),
            after_epoch: EpochId::start(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
        }
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        let epoch = unsafe { *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        epoch.after(self.after_epoch) && unsafe { self.fetch.visit_chunk(chunk_idx) }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        unsafe { self.fetch.touch_chunk(chunk_idx) }
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let epoch = unsafe { *self.entity_epochs.as_ptr().add(idx as usize) };
        epoch.after(self.after_epoch) && unsafe { self.fetch.visit_item(idx) }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> Self::Item {
        unsafe { self.fetch.get_item(idx) }
    }
}

macro_rules! modified_relation {
    ($query:ty, $bound:path, $component:expr) => {
        impl<R: $bound> AsQuery for Modified<$query> {
            type Query = Self;
        }

        impl<R: $bound> IntoQuery for Modified<$query> {
            #[inline]
            fn into_query(self) -> Self {
                self
            }
        }

        unsafe impl<R: $bound> Query for Modified<$query> {
            type Item<'a> = <$query as Query>::Item<'a>;
            type Fetch<'a> = ModifiedRelationFetch<<$query as Query>::Fetch<'a>>;

            const MUTABLE: bool = <$query as Query>::MUTABLE;
            const FILTERS_ENTITIES: bool = true;

            #[inline]
            fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
                match <$query as Query>::cache_visit_archetype(archetype) {
                    None | Some(true) => None,
                    Some(false) => Some(false),
                }
            }

            #[inline]
            fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
                let access = self.query.component_access(comp)?;
                if comp.id() == $component {
                    Ok(Some(access.unwrap_or(Access::Read)))
                } else {
                    Ok(access)
                }
            }

            #[inline]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                self.query.visit_archetype(archetype)
            }

            #[inline]
            unsafe fn visit_archetype_late(&self, archetype: &Archetype) -> bool {
                let component = unsafe { archetype.component($component).unwrap_unchecked() };
                let data = unsafe { component.data() };
                data.epoch.after(self.after_epoch)
                    && unsafe { self.query.visit_archetype_late(archetype) }
            }

            #[inline]
            unsafe fn access_archetype(
                &self,
                archetype: &Archetype,
                mut f: impl FnMut(TypeId, Access),
            ) {
                // Presence-only relation filters still read the modification epochs.
                let mut accessed = false;
                unsafe {
                    self.query.access_archetype(archetype, |id, access| {
                        accessed |= id == $component;
                        f(id, access);
                    })
                };
                if !accessed {
                    f($component, Access::Read);
                }
            }

            #[inline]
            unsafe fn fetch<'a>(
                &self,
                arch_idx: u32,
                archetype: &'a Archetype,
                epoch: EpochId,
            ) -> Self::Fetch<'a> {
                let component = unsafe { archetype.component($component).unwrap_unchecked() };
                let data = unsafe { component.data() };
                let entity_epochs =
                    unsafe { NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId) };
                let chunk_epochs =
                    unsafe { NonNull::new_unchecked(data.chunk_epochs.as_ptr() as *mut EpochId) };
                ModifiedRelationFetch {
                    fetch: unsafe { self.query.fetch(arch_idx, archetype, epoch) },
                    after_epoch: self.after_epoch,
                    entity_epochs,
                    chunk_epochs,
                }
            }
        }

        unsafe impl<R: $bound> ImmutableQuery for Modified<$query> where $query: ImmutableQuery {}
        unsafe impl<R: $bound> SendQuery for Modified<$query> where $query: SendQuery {}
    };
}

macro_rules! modified_relation_arg {
    ($query:ty, $bound:path) => {
        impl<R: $bound> QueryArg for Modified<$query>
        where
            $query: DefaultQuery + SendQuery,
        {
            #[inline]
            fn new() -> Self {
                Self {
                    after_epoch: EpochId::start(),
                    query: <$query>::default_query(),
                }
            }
            #[inline]
            fn after(&mut self, world: &World) {
                self.after_epoch = world.epoch();
            }
        }
    };
}

macro_rules! modified_relation_forms {
    ($family:ident, $bound:path, $component:expr) => {
        impl<R: $bound> AsQuery for Modified<$family<&R>> {
            type Query = Modified<$family<Read<R>>>;
        }
        impl<R: $bound> AsQuery for Modified<$family<&mut R>> {
            type Query = Modified<$family<Write<R>>>;
        }
        modified_relation!($family<With<R>>, $bound, $component);
        modified_relation!($family<Read<R>>, $bound, $component);
        modified_relation!($family<Write<R>>, $bound, $component);
        modified_relation_arg!($family<With<R>>, $bound);
        modified_relation_arg!($family<Read<R>>, $bound);
        modified_relation_arg!($family<Write<R>>, $bound);
    };
}

modified_relation_forms!(Relates, Relation, type_id::<OriginComponent<R>>());
modified_relation_forms!(
    RelatesExclusive,
    ExclusiveRelation,
    type_id::<OriginComponent<R>>()
);
modified_relation_forms!(
    Related,
    Relation,
    if R::SYMMETRIC {
        type_id::<OriginComponent<R>>()
    } else {
        type_id::<TargetComponent<R>>()
    }
);
modified_relation!(FilterRelates<R>, Relation, type_id::<OriginComponent<R>>());
modified_relation_arg!(FilterRelates<R>, Relation);
modified_relation!(
    FilterRelated<R>,
    Relation,
    if R::SYMMETRIC {
        type_id::<OriginComponent<R>>()
    } else {
        type_id::<TargetComponent<R>>()
    }
);
modified_relation_arg!(FilterRelated<R>, Relation);

impl<R: Relation> AsQuery for Modified<RelatesTo<&R>> {
    type Query = Modified<RelatesTo<Read<R>>>;
}
impl<R: Relation> AsQuery for Modified<RelatesTo<&mut R>> {
    type Query = Modified<RelatesTo<Write<R>>>;
}
modified_relation!(
    RelatesTo<Read<R>>,
    Relation,
    type_id::<OriginComponent<R>>()
);
modified_relation!(
    RelatesTo<Write<R>>,
    Relation,
    type_id::<OriginComponent<R>>()
);
modified_relation!(
    FilterRelatesTo<R>,
    Relation,
    type_id::<OriginComponent<R>>()
);
modified_relation!(
    FilterRelatedBy<R>,
    Relation,
    if R::SYMMETRIC {
        type_id::<OriginComponent<R>>()
    } else {
        type_id::<TargetComponent<R>>()
    }
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{component::Component, entity::EntityId, query::Entities};
    use alloc::vec::Vec;

    #[derive(Clone, Copy)]
    struct Marker;
    impl Component for Marker {}

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Edge(u32);
    impl Relation for Edge {}

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Parent(u32);
    impl Relation for Parent {
        const EXCLUSIVE: bool = true;
    }
    impl ExclusiveRelation for Parent {}

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Symmetric(u32);
    impl Relation for Symmetric {
        const SYMMETRIC: bool = true;
    }

    fn spawn(world: &mut World) -> EntityId {
        world.spawn_one(Marker).id()
    }

    #[test]
    fn modified_relation_reparent_and_cutoff() {
        let mut world = World::new();
        let child = spawn(&mut world);
        let first = spawn(&mut world);
        let second = spawn(&mut world);
        let before = world.epoch();
        world.insert_relation(child, Parent(1), first).unwrap();
        let view = world.view_with(Modified::<RelatesExclusive<Read<Parent>>>::new(before));
        let (value, target) = view.try_get(child).unwrap();
        assert_eq!(*value, Parent(1));
        assert_eq!(target.id(), first);
        drop(view);
        let cutoff = world.epoch();
        assert_eq!(
            world
                .view_with(Modified::<RelatesExclusive<With<Parent>>>::new(cutoff))
                .iter()
                .count(),
            0
        );
        world.insert_relation(child, Parent(1), second).unwrap();
        let view = world.view_with(Modified::<RelatesExclusive<Read<Parent>>>::new(cutoff));
        let (value, target) = view.try_get(child).unwrap();
        assert_eq!(*value, Parent(1));
        assert_eq!(target.id(), second);
        drop(view);
        assert!(
            world
                .view_with(Modified::<FilterRelates<Parent>>::new(cutoff))
                .try_get(child)
                .is_ok()
        );
        assert!(
            world
                .view_with(Modified::with_query(
                    cutoff,
                    FilterRelatesTo::<Parent>::new(first)
                ))
                .try_get(child)
                .is_err()
        );
        assert!(
            world
                .view_with(Modified::with_query(
                    cutoff,
                    FilterRelatesTo::<Parent>::new(second)
                ))
                .try_get(child)
                .is_ok()
        );
    }

    #[test]
    fn modified_relation_filters_entities_and_chunks() {
        let mut world = World::new();
        let target = spawn(&mut world);
        let origins: Vec<_> = (0..600)
            .map(|_| {
                let origin = spawn(&mut world);
                world.insert_relation(origin, Edge(1), target).unwrap();
                origin
            })
            .collect();
        let cutoff = world.epoch();
        world
            .get::<Relates<Write<Edge>>>(origins[300])
            .unwrap()
            .next()
            .unwrap()
            .0
            .0 = 2;
        let found: Vec<_> = world
            .view_with((Entities, Modified::<Relates<Read<Edge>>>::new(cutoff)))
            .iter()
            .map(|(entity, mut edges)| {
                assert_eq!(edges.next().unwrap().0.0, 2);
                entity.id()
            })
            .collect();
        assert_eq!(found, [origins[300]]);
        let after = world.epoch();
        let query = Modified::<Relates<Write<Edge>>>::new(cutoff);
        assert_eq!(world.view_with(query).iter_mut().count(), 1);
        assert_eq!(
            world
                .view_with(Modified::<Relates<With<Edge>>>::new(after))
                .iter()
                .count(),
            1
        );
        assert_eq!(
            world
                .view_with(Modified::<Relates<With<Edge>>>::new(world.epoch()))
                .iter()
                .count(),
            0
        );
    }

    #[test]
    fn modified_relation_set_and_bound_queries() {
        let mut world = World::new();
        let origin = spawn(&mut world);
        let first = spawn(&mut world);
        let second = spawn(&mut world);
        world.insert_relation(origin, Edge(1), first).unwrap();
        let cutoff = world.epoch();
        world.insert_relation(origin, Edge(2), second).unwrap();
        assert_eq!(
            world
                .view_with(Modified::<Relates<With<Edge>>>::new(cutoff))
                .try_get(origin)
                .unwrap()
                .count(),
            2
        );
        let query = Modified::with_query(cutoff, RelatesTo::<Read<Edge>>::new(first));
        assert_eq!(world.view_with(query).try_get(origin), Ok(&Edge(1)));
        assert!(
            world
                .view_with(Modified::with_query(
                    cutoff,
                    FilterRelatedBy::<Edge>::new(origin)
                ))
                .try_get(second)
                .is_ok()
        );
        let cutoff = world.epoch();
        world.insert_relation(origin, Edge(4), first).unwrap();
        assert_eq!(
            world
                .view_with(Modified::with_query(
                    cutoff,
                    RelatesTo::<Read<Edge>>::new(first)
                ))
                .try_get(origin),
            Ok(&Edge(4))
        );
        let cutoff = world.epoch();
        world
            .view_with(Modified::with_query(
                EpochId::start(),
                RelatesTo::<Write<Edge>>::new(first),
            ))
            .try_get_mut(origin)
            .unwrap()
            .0 = 3;
        assert_eq!(
            world
                .view_with(Modified::with_query(
                    cutoff,
                    RelatesTo::<Read<Edge>>::new(first)
                ))
                .try_get(origin),
            Ok(&Edge(3))
        );
    }

    #[test]
    fn modified_relation_incoming_mutation_is_directional() {
        let mut world = World::new();
        let origin = spawn(&mut world);
        let target = spawn(&mut world);
        let cutoff = world.epoch();
        world.insert_relation(origin, Edge(1), target).unwrap();
        assert_eq!(
            world
                .view_with(Modified::<Related<Read<Edge>>>::new(cutoff))
                .try_get(target)
                .unwrap()
                .count(),
            1
        );
        assert!(
            world
                .view_with(Modified::<FilterRelated<Edge>>::new(cutoff))
                .try_get(target)
                .is_ok()
        );
        let cutoff = world.epoch();
        world
            .view_with(Modified::<Related<Write<Edge>>>::new(EpochId::start()))
            .try_get_mut(target)
            .unwrap()
            .next()
            .unwrap()
            .0
            .0 = 2;
        assert_eq!(
            world
                .view_with(Modified::<Related<Read<Edge>>>::new(cutoff))
                .try_get(target)
                .unwrap()
                .next()
                .unwrap()
                .0,
            &Edge(2)
        );
        assert!(
            world
                .view_with(Modified::<Relates<Read<Edge>>>::new(cutoff))
                .try_get(origin)
                .is_err()
        );
    }

    #[test]
    fn modified_relation_symmetric_incoming_write() {
        let mut world = World::new();
        let a = spawn(&mut world);
        let b = spawn(&mut world);
        let cutoff = world.epoch();
        world.insert_relation(a, Symmetric(1), b).unwrap();
        assert_eq!(
            world
                .view_with(Modified::<Related<With<Symmetric>>>::new(cutoff))
                .iter()
                .count(),
            2
        );
        let cutoff = world.epoch();
        world
            .view_with(Modified::<Related<Write<Symmetric>>>::new(EpochId::start()))
            .try_get_mut(a)
            .unwrap()
            .next()
            .unwrap()
            .0
            .0 = 2;
        assert_eq!(
            world
                .view_with(Modified::<Related<Read<Symmetric>>>::new(cutoff))
                .iter()
                .count(),
            1
        );
        assert_eq!(
            world
                .view_with(Modified::<Relates<Read<Symmetric>>>::new(cutoff))
                .try_get(a)
                .unwrap()
                .next()
                .unwrap()
                .0,
            &Symmetric(2)
        );
        assert!(
            world
                .view_with(Modified::with_query(
                    cutoff,
                    FilterRelatedBy::<Symmetric>::new(b)
                ))
                .try_get(a)
                .is_ok()
        );
    }

    #[test]
    fn modified_relation_system_cutoff_and_exclusive_write() {
        let mut world = World::new();
        let child = spawn(&mut world);
        let parent = spawn(&mut world);
        world.insert_relation(child, Parent(1), parent).unwrap();
        use crate::{
            relation::ChildOf,
            system::{IntoSystem, System},
            view::View,
        };
        let system =
            |_: View<Modified<RelatesExclusive<&ChildOf>>>, _: View<Modified<Related<&Edge>>>| {};
        system.into_system().run_alone(&mut world);
        let mut query = <Modified<RelatesExclusive<Read<Parent>>> as QueryArg>::new();
        assert_eq!(world.view_with(query).iter_mut().count(), 1);
        query.after(&world);
        assert_eq!(world.view_with(query).iter().count(), 0);
        world
            .view_with(Modified::<RelatesExclusive<Write<Parent>>>::new(
                EpochId::start(),
            ))
            .try_get_mut(child)
            .unwrap()
            .0
            .0 = 2;
        assert_eq!(world.view_with(query).try_get(child).unwrap().0, &Parent(2));
    }
}
