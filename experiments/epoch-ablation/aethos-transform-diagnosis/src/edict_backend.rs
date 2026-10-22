use std::ops::Deref;

use aethos_util::math::{Angle, Pos, Vec};
use edict::{
    component::Component,
    entity::{AliveEntity, EntityBound},
    epoch::EpochId,
    query::{
        Alt, AsQuery, Cpy, Entities, IntoQuery, Modified, Not, Or, Or2, RefMut, With, WithEpoch,
        Without,
    },
    relation::{FilterRelated, FilterRelates, Related, Relates, RelatesExclusive, Relation},
    system::State,
    view::{View, ViewCell, ViewRef},
    world::World,
};

#[derive(Copy, Clone, Default, Component)]
pub struct Global {
    pos: Pos,
    angle: Angle,
}

#[derive(Copy, Clone, Default, Component)]
pub struct Local {
    offset: Vec<i32>,
    angle: Angle,
}

#[derive(Copy, Clone, Default, Relation)]
#[edict(owned, exclusive)]
pub struct ChildOf;

/// Marker component.
/// Touched mutably to mark entitys dirty.
#[derive(Copy, Clone, Default, Component)]
struct Dirty;

/// Marker component.
/// Touched mutably to mark that its parent was marked dirty and will be cleaned from it.
#[derive(Copy, Clone, Default, Component)]
struct ParentDirty;

#[edict::system::system]
pub fn sync_global(world: &mut World, mut last_epoch: State<EpochId>) {
    let comp_epoch = *last_epoch;

    let world = world.local();

    for (e, has_dirty, has_parent_dirty) in
        world.view::<(Entities, Option<With<Dirty>>, Option<With<ParentDirty>>)>()
    {
        if has_dirty.is_none() || has_parent_dirty.is_none() {
            world.insert_bundle_defer(e.id(), (Dirty, ParentDirty));
        }
    }

    world.run_deferred();

    let marker_epoch = world.epoch();

    let mut related = world.view::<Related<With<ChildOf>>>();
    let related = related.lock();

    {
        let mut dirty_view = world.view::<WithEpoch<&mut Dirty>>();
        let mut dirty_view = dirty_view.lock();
        let mut parent_dirty_view = world.view::<&mut ParentDirty>();
        let mut parent_dirty_view = parent_dirty_view.lock();

        let mut roots = world
            .view::<Entities>()
            .filter_modified::<Global>(comp_epoch)
            .filter(Not(Relates::<With<ChildOf>>));

        let roots = roots.lock();

        {
            for e in roots {
                let Some((Dirty, epoch)) = dirty_view.get_mut(e) else {
                    continue;
                };

                debug_assert!(
                    !epoch.after(marker_epoch),
                    "Root couldn't be marked earlier"
                );

                let children = related.get(e);

                if let Some(children) = children {
                    for child in children {
                        mark_parent_dirty_tree(
                            child,
                            &mut dirty_view,
                            &mut parent_dirty_view,
                            &related,
                            marker_epoch,
                        );
                    }
                }
            }
        }

        let mut subroots = world.view::<Entities>().filter(Or2::new(
            Modified::<With<Local>>::new(comp_epoch),
            Modified::<RelatesExclusive<With<ChildOf>>>::new(comp_epoch),
        ));
        let subroots = subroots.lock();

        for e in subroots {
            let Some((Dirty, epoch)) = dirty_view.get_mut(e) else {
                continue;
            };

            if epoch.after(marker_epoch) {
                // Children if any were already marked.
                continue;
            }

            let children = related.get(e);

            if let Some(children) = children {
                for child in children {
                    mark_parent_dirty_tree(
                        child,
                        &mut dirty_view,
                        &mut parent_dirty_view,
                        &related,
                        marker_epoch,
                    );
                }
            }
        }
    }

    {
        let mut locals = world.view::<Cpy<Local>>();
        let locals = locals.lock();

        let mut globals = world.view::<&mut Global>();
        let mut globals = globals.lock();

        let mut parents = world
            .view::<RelatesExclusive<With<ChildOf>>>()
            .filter(With::<Local>);
        let parents = parents.lock();

        for e in world
            .view::<Entities>()
            .filter(Modified::<With<Dirty>>::new(marker_epoch))
            .filter(Not(Modified::<With<ParentDirty>>::new(marker_epoch)))
        {
            if let Some(p) = parents.get(e) {
                let Some(parent) = globals.get_mut(p).copied() else {
                    continue;
                };

                update_dirty_tree(e, parent, &locals, &mut globals, &related);
            } else {
                let Some(global) = globals.get_mut(e).copied() else {
                    continue;
                };
                let Some(children) = related.get(e) else {
                    continue;
                };

                for child in children {
                    update_dirty_tree(child, global, &locals, &mut globals, &related);
                }
            }
        }
    }

    *last_epoch = world.epoch();
}

fn mark_parent_dirty_tree(
    e: EntityBound,
    dirty_view: &mut View<(WithEpoch<&mut Dirty>,)>,
    parent_dirty_view: &mut View<(&mut ParentDirty,)>,
    related: &View<(Related<With<ChildOf>>,)>,
    marker_epoch: EpochId,
) {
    parent_dirty_view.get_mut(e);

    let Some((Dirty, epoch)) = dirty_view.get_mut(e) else {
        return;
    };

    if epoch.after(marker_epoch) {
        // Children if any were already marked.
        return;
    }

    let children = related.get(e);

    if let Some(children) = children {
        for child in children {
            mark_parent_dirty_tree(child, dirty_view, parent_dirty_view, related, marker_epoch);
        }
    }
}

fn update_dirty_tree(
    e: impl AliveEntity,
    parent: Global,
    locals: &View<(Cpy<Local>,)>,
    globals: &mut View<(&mut Global,)>,
    related: &View<(Related<With<ChildOf>>,)>,
) {
    let Some(local) = locals.get(e) else {
        return;
    };
    let Some(global) = globals.get_mut(e) else {
        return;
    };

    global.pos = parent.pos + local.offset.as_f32().rotated(parent.angle).as_i32();
    global.angle = parent.angle + local.angle;

    let global = *global;

    if let Some(children) = related.get(e) {
        for child in children {
            update_dirty_tree(child, global, locals, globals, related);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use edict::{entity::EntityId, scheduler::Scheduler};

    fn assert_position(world: &mut World, entity: EntityId, x: i32, y: i32) {
        let global = world.get::<&Global>(entity).unwrap();
        assert_eq!((global.pos.x, global.pos.y), (x, y));
    }

    #[test]
    fn sync_global_chains() {
        for depth in 1..=4 {
            let mut world = World::new();
            let mut scheduler = Scheduler::new();
            scheduler.add_system(sync_global);

            let root = world
                .spawn_one(Global {
                    pos: Pos::default() + Vec::new(10, 20),
                    angle: Angle::default(),
                })
                .id();
            let mut chain = std::vec![root];
            for i in 1..depth {
                let child = world
                    .spawn((
                        Global::default(),
                        Local {
                            offset: Vec::new(2, 3),
                            angle: Angle::default(),
                        },
                    ))
                    .id();
                world.insert_relation(child, ChildOf, chain[i - 1]).unwrap();
                chain.push(child);
            }

            scheduler.run_sequential(&mut world);
            for (i, &entity) in chain.iter().enumerate() {
                assert_position(&mut world, entity, 10 + 2 * i as i32, 20 + 3 * i as i32);
            }

            world.get::<&mut Global>(root).unwrap().pos = Pos::default() + Vec::new(30, 40);
            scheduler.run_sequential(&mut world);
            for (i, &entity) in chain.iter().enumerate() {
                assert_position(&mut world, entity, 30 + 2 * i as i32, 40 + 3 * i as i32);
            }

            if depth > 1 {
                world.get::<&mut Global>(root).unwrap().pos = Pos::default() + Vec::new(50, 60);
                world.get::<&mut Local>(chain[1]).unwrap().offset = Vec::new(4, 6);
                if depth > 2 {
                    world.get::<&mut Local>(chain[depth - 1]).unwrap().offset = Vec::new(4, 6);
                }
                scheduler.run_sequential(&mut world);
                for (i, &entity) in chain.iter().enumerate() {
                    let steps =
                        i as i32 + i32::from(i > 0) + i32::from(depth > 2 && i == depth - 1);
                    assert_position(&mut world, entity, 50 + 2 * steps, 60 + 3 * steps);
                }
            }
        }
    }

    #[test]
    fn sync_global_rotation_and_reparenting() {
        let mut world = World::new();
        let mut scheduler = Scheduler::new();
        scheduler.add_system(sync_global);
        let root = world
            .spawn_one(Global {
                pos: Pos::default() + Vec::new(10, 20),
                angle: Angle::from_degrees(90.0),
            })
            .id();
        let child = world
            .spawn((
                Global::default(),
                Local {
                    offset: Vec::new(10, 0),
                    angle: Angle::new(0.25),
                },
            ))
            .id();
        let grandchild = world
            .spawn((
                Global::default(),
                Local {
                    offset: Vec::new(0, 0),
                    angle: Angle::new(0.125),
                },
            ))
            .id();
        world.insert_relation(child, ChildOf, root).unwrap();
        world.insert_relation(grandchild, ChildOf, child).unwrap();

        scheduler.run_sequential(&mut world);
        assert_position(&mut world, child, 10, 30);
        assert_position(&mut world, grandchild, 10, 30);
        assert_eq!(
            world.get::<&Global>(grandchild).unwrap().angle,
            Angle::from_degrees(90.0) + Angle::new(0.25) + Angle::new(0.125)
        );

        let other_root = world
            .spawn_one(Global {
                pos: Pos::default() + Vec::new(100, 200),
                angle: Angle::default(),
            })
            .id();
        scheduler.run_sequential(&mut world);
        world.insert_relation(child, ChildOf, other_root).unwrap();
        scheduler.run_sequential(&mut world);
        assert_position(&mut world, root, 10, 20);
        assert_position(&mut world, child, 110, 200);
        assert_position(&mut world, grandchild, 110, 200);
        assert_eq!(
            world.get::<&Global>(grandchild).unwrap().angle,
            Angle::new(0.375)
        );
    }
}


use crate::{Backend, Fixture, Level, LocalTransform, Transform};
use edict::{entity::EntityId, scheduler::Scheduler};

pub struct Adapter {
    world: World,
    scheduler: Scheduler,
    entities: std::vec::Vec<EntityId>,
}

impl Backend for Adapter {
    fn new(fixture: &Fixture) -> Self {
        fn spawn<const N: usize>(world: &mut World, node: &crate::Node) -> EntityId {
            let global = Global {
                pos: node.global.pos,
                angle: node.global.angle,
            };
            let entity = if node.parent.is_some() {
                world
                    .spawn((
                        global,
                        Local {
                            offset: node.local.offset,
                            angle: node.local.angle,
                        },
                        Dirty,
                        ParentDirty,
                        Level::<N>,
                    ))
                    .id()
            } else {
                world.spawn((global, Dirty, ParentDirty, Level::<N>)).id()
            };
            entity
        }
        let mut world = World::new();
        let mut entities = vec![None; fixture.nodes.len()];
        let depths = fixture.depths();
        for &index in &fixture.spawn_order {
            let node = &fixture.nodes[index];
            let entity = match depths[index] {
                0 => spawn::<0>(&mut world, node),
                1 => spawn::<1>(&mut world, node),
                2 => spawn::<2>(&mut world, node),
                3 => spawn::<3>(&mut world, node),
                depth => panic!("unsupported fixture depth {depth}"),
            };
            entities[index] = Some(entity);
        }
        let entities: std::vec::Vec<_> = entities.into_iter().map(Option::unwrap).collect();
        for (index, node) in fixture.nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                world
                    .insert_relation(entities[index], ChildOf, entities[parent])
                    .unwrap();
            }
        }
        let mut scheduler = Scheduler::new();
        scheduler.add_system(sync_global);
        Self {
            world,
            scheduler,
            entities,
        }
    }

    fn set_root(&mut self, index: usize, value: Transform) {
        *self.world.get::<&mut Global>(self.entities[index]).unwrap() = Global {
            pos: value.pos,
            angle: value.angle,
        };
    }

    fn set_local(&mut self, index: usize, value: LocalTransform) {
        *self.world.get::<&mut Local>(self.entities[index]).unwrap() = Local {
            offset: value.offset,
            angle: value.angle,
        };
    }

    fn reparent(&mut self, index: usize, parent: usize) {
        self.world
            .insert_relation(self.entities[index], ChildOf, self.entities[parent])
            .unwrap();
    }

    fn sync(&mut self) {
        self.scheduler.run_sequential(&mut self.world);
    }

    fn global(&mut self, index: usize) -> Transform {
        let global = self.world.get::<&Global>(self.entities[index]).unwrap();
        Transform {
            pos: global.pos,
            angle: global.angle,
        }
    }
}

#[cfg(test)]
mod level_tests {
    use super::*;

    #[test]
    fn hierarchy_levels_have_distinct_components() {
        let mut fixture = crate::forest(85);
        fixture.spawn_order.reverse();
        let mut adapter = Adapter::new(&fixture);
        assert_eq!(adapter.world.view::<&Level<0>>().iter().count(), 1);
        assert_eq!(adapter.world.view::<&Level<1>>().iter().count(), 4);
        assert_eq!(adapter.world.view::<&Level<2>>().iter().count(), 16);
        assert_eq!(adapter.world.view::<&Level<3>>().iter().count(), 64);
        for (index, depth) in fixture.depths().into_iter().enumerate() {
            let entity = adapter.entities[index];
            let markers = [
                adapter.world.get::<&Level<0>>(entity).is_ok(),
                adapter.world.get::<&Level<1>>(entity).is_ok(),
                adapter.world.get::<&Level<2>>(entity).is_ok(),
                adapter.world.get::<&Level<3>>(entity).is_ok(),
            ];
            assert_eq!(markers.iter().filter(|&&present| present).count(), 1);
            assert!(markers[depth], "node {index}, depth {depth}");
        }
    }
}
