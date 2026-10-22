//! Bevy-style candidate collection and pass markers with native edict systems.

use edict::{
    entity::EntityId,
    query::{Alt, Entities, Modified, Not, With},
    relation::{ChildOf, Related, RelatesExclusive},
    scheduler::Scheduler,
    system::State,
    view::{View, ViewCell},
    world::World,
};

use crate::{Backend, Fixture, Level, LocalTransform, Transform, compose};

use crate::edict_bevy_backend::{Global, LocalPosition, Marks};

#[derive(Default)]
struct Scratch {
    pass: u64,
    candidates: Vec<EntityId>,
}

type Hierarchy<'w> = View<
    'w,
    (
        Option<RelatesExclusive<With<ChildOf>>>,
        Option<Related<With<ChildOf>>>,
        Option<&'static LocalPosition>,
    ),
>;

fn cover(entity: EntityId, pass: u64, markers: &mut View<Alt<Marks>>, hierarchy: &Hierarchy) {
    let Ok(mut marker) = markers.try_get_mut(entity) else {
        return;
    };
    marker.covered = pass;
    if marker.dirty == pass {
        return;
    }
    marker.dirty = pass;
    if let Ok((_, Some(children), _)) = hierarchy.try_get(entity) {
        for child in children {
            cover(child.id(), pass, markers, hierarchy);
        }
    }
}

fn mark(entity: EntityId, pass: u64, markers: &mut View<Alt<Marks>>, hierarchy: &Hierarchy) {
    let Ok(mut marker) = markers.try_get_mut(entity) else {
        return;
    };
    if marker.dirty == pass {
        return;
    }
    marker.dirty = pass;
    if let Ok((_, Some(children), _)) = hierarchy.try_get(entity) {
        for child in children {
            cover(child.id(), pass, markers, hierarchy);
        }
    }
}

fn update(
    entity: EntityId,
    parent: Transform,
    hierarchy: &Hierarchy,
    globals: &mut View<Alt<Global>>,
) {
    let Ok((_, children, Some(local))) = hierarchy.try_get(entity) else {
        return;
    };
    let Ok(mut global) = globals.try_get_mut(entity) else {
        return;
    };
    let value = compose(parent, local.0);
    global.0 = value;
    if let Some(children) = children {
        for child in children {
            update(child.id(), value, hierarchy, globals);
        }
    }
}

fn sync_global(
    mut scratch: State<Scratch>,
    mut markers: View<Alt<Marks>>,
    hierarchy: Hierarchy,
    changed_locals: View<Entities, Modified<With<LocalPosition>>>,
    changed_parents: View<Entities, Modified<RelatesExclusive<With<ChildOf>>>>,
    mut changed_roots: ViewCell<
        Entities,
        (Modified<With<Global>>, Not<RelatesExclusive<With<ChildOf>>>),
    >,
    mut globals: ViewCell<Alt<Global>>,
) {
    #[cfg(feature = "phases")]
    let phase_start = std::time::Instant::now();
    scratch.pass = scratch.pass.checked_add(1).expect("sync pass overflow");
    let pass = scratch.pass;
    scratch.candidates.clear();
    scratch
        .candidates
        .extend(changed_roots.iter().map(|entity| entity.id()));
    changed_roots.unlock();
    for &entity in &scratch.candidates {
        mark(entity, pass, &mut markers, &hierarchy);
    }
    for entity in changed_locals.iter().chain(changed_parents.iter()) {
        if markers
            .try_get_mut(entity)
            .is_ok_and(|marker| marker.dirty == pass)
        {
            continue;
        }
        mark(entity.id(), pass, &mut markers, &hierarchy);
        scratch.candidates.push(entity.id());
    }
    #[cfg(feature = "phases")]
    let marked = phase_start.elapsed();
    let mut globals = globals.lock();
    for &entity in &scratch.candidates {
        if markers
            .try_get_mut(entity)
            .is_ok_and(|marker| marker.covered == pass)
        {
            continue;
        }
        let Ok((parent, children, local)) = hierarchy.try_get(entity) else {
            continue;
        };
        if let (Some(parent), Some(_)) = (parent, local) {
            let Ok(global) = globals.try_get_mut(parent) else {
                continue;
            };
            let parent = global.0;
            update(entity, parent, &hierarchy, &mut globals);
        } else if let Some(children) = children {
            let Ok(mut global) = globals.try_get_mut(entity) else {
                continue;
            };
            let parent = global.0;
            for child in children {
                update(child.id(), parent, &hierarchy, &mut globals);
            }
        }
    }
    #[cfg(feature = "phases")]
    {
        MARK_NS.store(marked.as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        UPDATE_NS.store((phase_start.elapsed() - marked).as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_sync_does_not_write_globals() {
        let fixture = crate::forest(170);
        let mut backend = Adapter::new(&fixture);
        backend.sync();
        for mutate in [false, true] {
            if mutate {
                backend.set_root(0, fixture.nodes[85].global);
                backend.set_local(1, fixture.nodes[2].local);
                backend.reparent(2, 85);
                backend.sync();
            }
            let epoch = backend.world.epoch();
            backend.sync();
            assert_eq!(
                backend
                    .world
                    .view::<Entities>()
                    .filter(Modified::<With<Global>>::new(epoch))
                    .iter()
                    .count(),
                0,
                "idle sync must not write globals after mutate={mutate}"
            );
        }
    }
}

pub struct Adapter {
    world: World,
    scheduler: Scheduler,
    entities: Vec<EntityId>,
}

impl Backend for Adapter {
    fn new(fixture: &Fixture) -> Self {
        fn spawn<const N: usize>(world: &mut World, node: &crate::Node) -> EntityId {
            let entity = if node.parent.is_some() {
                world
                    .spawn((
                        Global(node.global),
                        LocalPosition(node.local),
                        Marks::default(),
                        Level::<N>,
                    ))
                    .id()
            } else {
                world.spawn((Global(node.global), Marks::default(), Level::<N>)).id()
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
        let entities: Vec<_> = entities.into_iter().map(Option::unwrap).collect();
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
        self.world
            .get::<&mut Global>(self.entities[index])
            .unwrap()
            .0 = value;
    }

    fn set_local(&mut self, index: usize, value: LocalTransform) {
        self.world
            .get::<&mut LocalPosition>(self.entities[index])
            .unwrap()
            .0 = value;
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
        self.world.get::<&Global>(self.entities[index]).unwrap().0
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

#[cfg(feature = "phases")]
static MARK_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "phases")]
static UPDATE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn phases() -> (u64, u64) {
    #[cfg(feature = "phases")]
    return (MARK_NS.load(std::sync::atomic::Ordering::Relaxed), UPDATE_NS.load(std::sync::atomic::Ordering::Relaxed));
    #[cfg(not(feature = "phases"))]
    (0, 0)
}
