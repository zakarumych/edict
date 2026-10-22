use crate::{Backend, Fixture, Level, LocalTransform, Node, Transform, compose};
use shipyard::{Component, EntityId, Get, IntoIter, View, ViewMut, World};

#[derive(Component, Copy, Clone)]
#[track(Insertion, Modification)]
struct Global(Transform);
#[derive(Component, Copy, Clone)]
#[track(Insertion, Modification)]
struct Local(LocalTransform);
#[derive(Component, Copy, Clone)]
#[track(Insertion, Modification)]
struct Parent(EntityId);
#[derive(Component, Default)]
struct Children(Vec<EntityId>);
#[derive(Component, Default)]
struct Marks {
    dirty: u64,
    parent: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_level<const N: usize>(adapter: &Adapter, fixture: &Fixture, count: usize) {
        let levels = adapter.world.borrow::<View<Level<N>>>().unwrap();
        assert_eq!(levels.iter().count(), count);
        for (index, depth) in fixture.depths().into_iter().enumerate() {
            assert_eq!(levels.get(adapter.entities[index]).is_ok(), depth == N);
        }
    }

    #[test]
    fn hierarchy_levels_have_distinct_components() {
        let mut fixture = crate::forest(85);
        fixture.spawn_order.reverse();
        let adapter = Adapter::new(&fixture);
        assert_level::<0>(&adapter, &fixture, 1);
        assert_level::<1>(&adapter, &fixture, 4);
        assert_level::<2>(&adapter, &fixture, 16);
        assert_level::<3>(&adapter, &fixture, 64);
    }
}

pub struct Adapter {
    world: World,
    entities: Vec<EntityId>,
    frame: u64,
    candidates: Vec<EntityId>,
}

fn mark(
    entity: EntityId,
    covered: bool,
    frame: u64,
    marks: &mut ViewMut<Marks>,
    children: &View<Children>,
) -> bool {
    let mut marker = (&mut *marks).get(entity).unwrap();
    if covered {
        marker.parent = frame;
    }
    if marker.dirty == frame {
        return false;
    }
    marker.dirty = frame;
    for &child in &children.get(entity).unwrap().0 {
        mark(child, true, frame, marks, children);
    }
    true
}

fn update(
    entity: EntityId,
    parent: Transform,
    locals: &ViewMut<Local>,
    globals: &mut ViewMut<Global>,
    children: &View<Children>,
) {
    let global = compose(parent, locals.get(entity).unwrap().0);
    (&mut *globals).get(entity).unwrap().0 = global;
    for &child in &children.get(entity).unwrap().0 {
        update(child, global, locals, globals, children);
    }
}

fn spawn<const N: usize>(world: &mut World, node: &Node) -> EntityId {
    if node.parent.is_some() {
        world.add_entity((
            Global(node.global),
            Children::default(),
            Marks::default(),
            Local(node.local),
            Level::<N>,
        ))
    } else {
        world.add_entity((
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
                world.add_component(entities[index], (Parent(entities[parent]),));
                let mut children = world.borrow::<ViewMut<Children>>().unwrap();
                (&mut children)
                    .get(entities[parent])
                    .unwrap()
                    .0
                    .push(entities[index]);
            }
        }
        Self {
            world,
            entities,
            frame: 0,
            candidates: Vec::new(),
        }
    }

    fn set_root(&mut self, index: usize, value: Transform) {
        let mut globals = self.world.borrow::<ViewMut<Global>>().unwrap();
        (&mut globals).get(self.entities[index]).unwrap().0 = value;
    }
    fn set_local(&mut self, index: usize, value: LocalTransform) {
        let mut locals = self.world.borrow::<ViewMut<Local>>().unwrap();
        (&mut locals).get(self.entities[index]).unwrap().0 = value;
    }
    fn reparent(&mut self, index: usize, parent: usize) {
        let entity = self.entities[index];
        let parent = self.entities[parent];
        let (mut parents, mut children) = self
            .world
            .borrow::<(ViewMut<Parent>, ViewMut<Children>)>()
            .unwrap();
        let previous = parents.get(entity).unwrap().0;
        (&mut children)
            .get(previous)
            .unwrap()
            .0
            .retain(|&child| child != entity);
        (&mut children).get(parent).unwrap().0.push(entity);
        (&mut parents).get(entity).unwrap().0 = parent;
    }
    fn sync(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        let (mut globals, locals, parents, children, mut marks) = self
            .world
            .borrow::<(
                ViewMut<Global>,
                ViewMut<Local>,
                ViewMut<Parent>,
                View<Children>,
                ViewMut<Marks>,
            )>()
            .unwrap();
        let changed = &mut self.candidates;
        changed.clear();
        for (entity, _) in globals.inserted_or_modified().iter().with_id() {
            if parents.get(entity).is_err() {
                changed.push(entity);
            }
        }
        changed.extend(
            locals
                .inserted_or_modified()
                .iter()
                .with_id()
                .map(|(entity, _)| entity),
        );
        changed.extend(
            parents
                .inserted_or_modified()
                .iter()
                .with_id()
                .map(|(entity, _)| entity),
        );
        changed.retain(|&entity| mark(entity, false, frame, &mut marks, &children));
        for &entity in changed.iter() {
            let marker = (&marks).get(entity).unwrap();
            if marker.parent == frame {
                continue;
            }
            if let Ok(parent) = parents.get(entity) {
                let global = (&globals).get(parent.0).unwrap().0;
                update(entity, global, &locals, &mut globals, &children);
            } else {
                let global = (&globals).get(entity).unwrap().0;
                for &child in &children.get(entity).unwrap().0 {
                    update(child, global, &locals, &mut globals, &children);
                }
            }
        }
        globals.clear_all_inserted_and_modified();
        locals.clear_all_inserted_and_modified();
        parents.clear_all_inserted_and_modified();
    }
    fn global(&mut self, index: usize) -> Transform {
        self.world
            .borrow::<View<Global>>()
            .unwrap()
            .get(self.entities[index])
            .unwrap()
            .0
    }
}
