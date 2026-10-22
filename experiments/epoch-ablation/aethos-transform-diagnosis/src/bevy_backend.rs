//! Bevy native change ticks and hierarchy relationship hooks.

use bevy_ecs::{prelude::*, schedule::SingleThreadedExecutor};

use crate::{Backend, Fixture, Level, LocalTransform, Node, Transform, compose};

#[derive(Component)]
struct Global(Transform);

#[derive(Component)]
struct LocalPosition(LocalTransform);

#[derive(Component, Default)]
struct Marks {
    dirty: u64,
    covered: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_level<const N: usize>(adapter: &mut Adapter, fixture: &Fixture, count: usize) {
        assert_eq!(
            adapter
                .world
                .query::<&Level<N>>()
                .iter(&adapter.world)
                .count(),
            count
        );
        for (index, depth) in fixture.depths().into_iter().enumerate() {
            assert_eq!(
                adapter
                    .world
                    .get::<Level<N>>(adapter.entities[index])
                    .is_some(),
                depth == N
            );
        }
    }

    #[test]
    fn hierarchy_levels_have_distinct_archetypes() {
        let mut fixture = crate::forest(85);
        fixture.spawn_order.reverse();
        let mut adapter = Adapter::new(&fixture);
        assert_level::<0>(&mut adapter, &fixture, 1);
        assert_level::<1>(&mut adapter, &fixture, 4);
        assert_level::<2>(&mut adapter, &fixture, 16);
        assert_level::<3>(&mut adapter, &fixture, 64);
        let archetypes: std::collections::HashSet<_> = adapter
            .entities
            .iter()
            .map(|&entity| adapter.world.entity(entity).archetype().id())
            .collect();
        assert_eq!(archetypes.len(), 4);
    }
}

#[derive(Default)]
struct Scratch {
    pass: u64,
    candidates: Vec<Entity>,
}

type Hierarchy<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static ChildOf>,
        Option<&'static Children>,
        Option<&'static LocalPosition>,
    ),
>;

fn cover(entity: Entity, pass: u64, markers: &mut Query<&mut Marks>, hierarchy: &Hierarchy) {
    let Ok(mut marker) = markers.get_mut(entity) else {
        return;
    };
    marker.covered = pass;
    if marker.dirty == pass {
        return;
    }
    marker.dirty = pass;
    if let Ok((_, Some(children), _)) = hierarchy.get(entity) {
        for child in children.iter() {
            cover(child, pass, markers, hierarchy);
        }
    }
}

fn mark(entity: Entity, pass: u64, markers: &mut Query<&mut Marks>, hierarchy: &Hierarchy) {
    let Ok(mut marker) = markers.get_mut(entity) else {
        return;
    };
    if marker.dirty == pass {
        return;
    }
    marker.dirty = pass;
    if let Ok((_, Some(children), _)) = hierarchy.get(entity) {
        for child in children.iter() {
            cover(child, pass, markers, hierarchy);
        }
    }
}

fn update(
    entity: Entity,
    parent: Transform,
    hierarchy: &Hierarchy,
    globals: &mut Query<&mut Global>,
) {
    let Ok((_, children, Some(local))) = hierarchy.get(entity) else {
        return;
    };
    let Ok(mut global) = globals.get_mut(entity) else {
        return;
    };
    let value = compose(parent, local.0);
    global.0 = value;
    if let Some(children) = children {
        for child in children.iter() {
            update(child, value, hierarchy, globals);
        }
    }
}

fn sync_global(
    mut scratch: Local<Scratch>,
    mut markers: Query<&mut Marks>,
    hierarchy: Hierarchy,
    changed_children: Query<Entity, Or<(Changed<LocalPosition>, Changed<ChildOf>)>>,
    mut globals: ParamSet<(
        Query<Entity, (Changed<Global>, Without<ChildOf>)>,
        Query<&mut Global>,
    )>,
) {
    #[cfg(feature = "phases")]
    let phase_start = std::time::Instant::now();
    scratch.pass = scratch.pass.checked_add(1).expect("sync pass overflow");
    let pass = scratch.pass;
    scratch.candidates.clear();
    scratch.candidates.extend(globals.p0().iter());
    for &entity in &scratch.candidates {
        mark(entity, pass, &mut markers, &hierarchy);
    }
    for entity in changed_children.iter() {
        if markers.get(entity).is_ok_and(|marker| marker.dirty == pass) {
            continue;
        }
        mark(entity, pass, &mut markers, &hierarchy);
        scratch.candidates.push(entity);
    }
    #[cfg(feature = "phases")]
    let marked = phase_start.elapsed();
    let mut globals = globals.p1();
    for &entity in &scratch.candidates {
        if markers
            .get(entity)
            .is_ok_and(|marker| marker.covered == pass)
        {
            continue;
        }
        let Ok((parent, children, local)) = hierarchy.get(entity) else {
            continue;
        };
        if let (Some(parent), Some(_)) = (parent, local) {
            let Ok(global) = globals.get(parent.parent()) else {
                continue;
            };
            let parent = global.0;
            update(entity, parent, &hierarchy, &mut globals);
        } else if let Some(children) = children {
            let Ok(global) = globals.get(entity) else {
                continue;
            };
            let parent = global.0;
            for child in children.iter() {
                update(child, parent, &hierarchy, &mut globals);
            }
        }
    }
    #[cfg(feature = "phases")]
    {
        MARK_NS.store(marked.as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        UPDATE_NS.store((phase_start.elapsed() - marked).as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    }
}

pub struct Adapter {
    world: World,
    schedule: Schedule,
    entities: Vec<Entity>,
}

fn spawn<const N: usize>(world: &mut World, node: &Node) -> Entity {
    if node.parent.is_some() {
        world
            .spawn((
                Global(node.global),
                Marks::default(),
                LocalPosition(node.local),
                Level::<N>,
            ))
            .id()
    } else {
        world
            .spawn((Global(node.global), Marks::default(), Level::<N>))
            .id()
    }
}

impl Backend for Adapter {
    fn new(fixture: &Fixture) -> Self {
        let mut world = World::new();
        let mut entities = vec![Entity::PLACEHOLDER; fixture.nodes.len()];
        let depths = fixture.depths();
        for &index in &fixture.spawn_order {
            let node = &fixture.nodes[index];
            entities[index] = match depths[index] {
                0 => spawn::<0>(&mut world, node),
                1 => spawn::<1>(&mut world, node),
                2 => spawn::<2>(&mut world, node),
                3 => spawn::<3>(&mut world, node),
                depth => panic!("unsupported hierarchy depth {depth}"),
            };
        }
        for (index, node) in fixture.nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                world
                    .entity_mut(entities[index])
                    .insert(ChildOf(entities[parent]));
            }
        }
        let mut schedule = Schedule::default();
        schedule.set_executor(SingleThreadedExecutor::new());
        schedule.add_systems(sync_global);
        Self {
            world,
            schedule,
            entities,
        }
    }

    fn set_root(&mut self, index: usize, value: Transform) {
        self.world
            .get_mut::<Global>(self.entities[index])
            .unwrap()
            .0 = value;
    }

    fn set_local(&mut self, index: usize, value: LocalTransform) {
        self.world
            .get_mut::<LocalPosition>(self.entities[index])
            .unwrap()
            .0 = value;
    }

    fn reparent(&mut self, index: usize, parent: usize) {
        self.world
            .entity_mut(self.entities[index])
            .insert(ChildOf(self.entities[parent]));
    }

    fn sync(&mut self) {
        self.schedule.run(&mut self.world);
        self.world.clear_trackers();
    }

    fn global(&mut self, index: usize) -> Transform {
        self.world.get::<Global>(self.entities[index]).unwrap().0
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
