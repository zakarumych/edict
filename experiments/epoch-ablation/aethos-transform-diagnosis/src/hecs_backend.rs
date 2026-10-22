use crate::{Backend, Fixture, Level, LocalTransform, Node, Transform, compose};
use hecs::{ChangeTracker, Component, Entity, World};

#[derive(Copy, Clone, PartialEq)]
struct Global(Transform);
#[derive(Copy, Clone, PartialEq)]
struct Local(LocalTransform);
#[derive(Copy, Clone, PartialEq)]
struct Parent(Entity);
#[derive(Default)]
struct Children(Vec<Entity>);
#[derive(Default)]
struct Marks {
    dirty: u64,
    parent: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_level<const N: usize>(adapter: &Adapter, fixture: &Fixture, count: usize) {
        assert_eq!(adapter.world.query::<&Level<N>>().iter().count(), count);
        for (index, depth) in fixture.depths().into_iter().enumerate() {
            assert_eq!(
                adapter
                    .world
                    .get::<&Level<N>>(adapter.entities[index])
                    .is_ok(),
                depth == N
            );
        }
    }

    #[test]
    fn hierarchy_levels_have_distinct_archetypes() {
        let mut fixture = crate::forest(85);
        fixture.spawn_order.reverse();
        let adapter = Adapter::new(&fixture);
        assert_level::<0>(&adapter, &fixture, 1);
        assert_level::<1>(&adapter, &fixture, 4);
        assert_level::<2>(&adapter, &fixture, 16);
        assert_level::<3>(&adapter, &fixture, 64);
        assert_eq!(
            adapter
                .world
                .archetypes()
                .filter(|archetype| archetype.len() != 0)
                .count(),
            4
        );
    }
}

pub struct Adapter {
    world: World,
    entities: Vec<Entity>,
    globals: ChangeTracker<Global>,
    locals: ChangeTracker<Local>,
    parents: ChangeTracker<Parent>,
    frame: u64,
    candidates: Vec<Entity>,
}

fn changed<T: Component + Clone + PartialEq>(
    tracker: &mut ChangeTracker<T>,
    world: &mut World,
    output: &mut Vec<Entity>,
) {
    let mut changes = tracker.track(world);
    output.extend(changes.added().map(|(entity, _)| entity));
    output.extend(changes.changed().map(|(entity, _, _)| entity));
}

fn mark(world: &World, entity: Entity, covered: bool, frame: u64) -> bool {
    {
        let mut marker = world.get::<&mut Marks>(entity).unwrap();
        if covered {
            marker.parent = frame;
        }
        if marker.dirty == frame {
            return false;
        }
        marker.dirty = frame;
    }
    for &child in &world.get::<&Children>(entity).unwrap().0 {
        mark(world, child, true, frame);
    }
    true
}

fn update(world: &World, entity: Entity, parent: Transform) {
    let global = compose(parent, world.get::<&Local>(entity).unwrap().0);
    world.get::<&mut Global>(entity).unwrap().0 = global;
    for &child in &world.get::<&Children>(entity).unwrap().0 {
        update(world, child, global);
    }
}

fn spawn<const N: usize>(world: &mut World, node: &Node) -> Entity {
    if node.parent.is_some() {
        world.spawn((
            Global(node.global),
            Children::default(),
            Marks::default(),
            Local(node.local),
            Level::<N>,
        ))
    } else {
        world.spawn((
            Global(node.global),
            Children::default(),
            Marks::default(),
            Level::<N>,
        ))
    }
}

impl Backend for Adapter {
    fn new(fixture: &Fixture) -> Self {
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
                depth => panic!("unsupported hierarchy depth {depth}"),
            };
            entities[index] = Some(entity);
        }
        let entities: Vec<_> = entities.into_iter().map(Option::unwrap).collect();
        for (index, node) in fixture.nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                world
                    .insert_one(entities[index], Parent(entities[parent]))
                    .unwrap();
                world
                    .get::<&mut Children>(entities[parent])
                    .unwrap()
                    .0
                    .push(entities[index]);
            }
        }
        Self {
            world,
            entities,
            globals: ChangeTracker::new(),
            locals: ChangeTracker::new(),
            parents: ChangeTracker::new(),
            frame: 0,
            candidates: Vec::new(),
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
            .get::<&mut Local>(self.entities[index])
            .unwrap()
            .0 = value;
    }
    fn reparent(&mut self, index: usize, parent: usize) {
        let entity = self.entities[index];
        let parent = self.entities[parent];
        let previous = self.world.get::<&Parent>(entity).unwrap().0;
        self.world
            .get::<&mut Children>(previous)
            .unwrap()
            .0
            .retain(|&child| child != entity);
        self.world
            .get::<&mut Children>(parent)
            .unwrap()
            .0
            .push(entity);
        self.world.get::<&mut Parent>(entity).unwrap().0 = parent;
    }
    fn sync(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        let candidates = &mut self.candidates;
        candidates.clear();
        changed(&mut self.globals, &mut self.world, candidates);
        candidates.retain(|&entity| self.world.get::<&Parent>(entity).is_err());
        changed(&mut self.locals, &mut self.world, candidates);
        changed(&mut self.parents, &mut self.world, candidates);
        candidates.retain(|&entity| mark(&self.world, entity, false, frame));
        for &entity in candidates.iter() {
            if self.world.get::<&Marks>(entity).unwrap().parent == frame {
                continue;
            }
            let parent = self
                .world
                .get::<&Parent>(entity)
                .ok()
                .map(|parent| parent.0);
            if let Some(parent) = parent {
                let global = self.world.get::<&Global>(parent).unwrap().0;
                update(&self.world, entity, global);
            } else {
                let global = self.world.get::<&Global>(entity).unwrap().0;
                for &child in &self.world.get::<&Children>(entity).unwrap().0 {
                    update(&self.world, child, global);
                }
            }
        }
        // Propagated outputs belong to this pass's baseline.
        if !candidates.is_empty() {
            drop(self.globals.track(&mut self.world));
        }
    }
    fn global(&mut self, index: usize) -> Transform {
        self.world.get::<&Global>(self.entities[index]).unwrap().0
    }
}
