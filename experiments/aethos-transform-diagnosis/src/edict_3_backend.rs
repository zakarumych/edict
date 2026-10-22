use aethos_util::math::{Angle, Pos, Vec};
use edict::{
    action::LocalActionEncoder,
    component::Component,
    entity::{AliveEntity, EntityBound},
    epoch::{EpochCounter, EpochId},
    query::{Cpy, Entities, Modified, Not, Or2, With, WithEpoch, Without},
    relation::{Related, Relates, RelatesExclusive, Relation},
    resources::{Res, ResMut},
    system::State,
    view::{View, ViewRef},
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
/// Touched mutably to mark entity position as dirty.
#[derive(Copy, Clone, Default, Component)]
struct Dirty {
    parent_dirty: bool,
}

#[edict::system::system]
pub fn add_markers(world: &mut World) {
    let world = world.local();
    for e in world.view_filter::<Entities, Without<Dirty>>() {
        world.insert_defer(e, Dirty::default());
    }

    world.run_deferred();
}

struct MarkerEpoch(EpochId);

#[edict::system::system]
pub fn traverse(
    roots: View<Entities, (Modified<With<Global>>, Not<RelatesExclusive<With<ChildOf>>>)>,
    subroots: View<Entities, Or2<Modified<With<Local>>, Modified<RelatesExclusive<With<ChildOf>>>>>,
    related: View<Related<With<ChildOf>>>,
    mut dirty_view: View<WithEpoch<&mut Dirty>>,
    this_epoch: EpochId,
) {
    for e in roots {
        let Some((dirty, epoch)) = dirty_view.get_mut(e) else {
            continue;
        };

        dirty.parent_dirty = false;

        if let Some(children) = related.get(e) {
            for child in children {
                mark_parent_dirty_tree(child, &mut dirty_view, &related, this_epoch);
            }
        }
    }

    for e in subroots {
        let Some((dirty, epoch)) = dirty_view.get_mut(e) else {
            continue;
        };

        if !epoch.before(this_epoch) {
            // Already marked subtree.
            continue;
        }

        dirty.parent_dirty = false;

        if let Some(children) = related.get(e) {
            for child in children {
                mark_parent_dirty_tree(child, &mut dirty_view, &related, this_epoch);
            }
        }
    }
}

#[edict::system::system]
pub fn update(
    marked: View<(Entities, Modified<&Dirty>)>,
    locals: View<Cpy<Local>>,
    mut globals: View<&mut Global>,
    relates: View<RelatesExclusive<With<ChildOf>>>,
    related: View<Related<With<ChildOf>>>,
) {
    for (e, dirty) in marked {
        if dirty.parent_dirty {
            continue;
        }

        if let Some(parent) = relates.get(e) {
            let Some(parent) = globals.get_mut(parent).copied() else {
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

fn mark_parent_dirty_tree(
    e: EntityBound,
    dirty_view: &mut View<WithEpoch<&mut Dirty>>,
    related: &View<Related<With<ChildOf>>>,
    marker_epoch: EpochId,
) {
    let Some((dirty, epoch)) = dirty_view.get_mut(e) else {
        return;
    };

    dirty.parent_dirty = true;

    if epoch.after(marker_epoch) {
        // Children if any were already marked.
        return;
    }

    let children = related.get(e);

    if let Some(children) = children {
        for child in children {
            mark_parent_dirty_tree(child, dirty_view, related, marker_epoch);
        }
    }
}

fn update_dirty_tree(
    e: impl AliveEntity,
    parent: Global,
    locals: &View<Cpy<Local>>,
    globals: &mut View<&mut Global>,
    related: &View<Related<With<ChildOf>>>,
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
                        Dirty::default(),
                        Level::<N>,
                    ))
                    .id()
            } else {
                world.spawn((global, Dirty::default(), Level::<N>)).id()
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
        scheduler.add_system(add_markers);
        scheduler.add_system(traverse);
        scheduler.add_system(update);
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

pub fn idle_writes(count: usize) -> [usize; 3] {
    let mut backend = Adapter::new(&crate::forest(count));
    backend.sync();
    std::array::from_fn(|_| {
        let epoch = backend.world.epoch();
        backend.sync();
        backend.world.view::<Entities>()
            .filter(Modified::<With<Global>>::new(epoch)).iter().count()
    })
}
