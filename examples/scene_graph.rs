//! A small 2D scene graph: inherited poses, reparenting, and owned subtrees.
//! Run with `cargo run --example scene_graph`.

use std::{collections::BTreeMap, f64::consts::FRAC_PI_2};

use edict::{
    bundle::EntityBuilder,
    component::Component,
    entity::EntityId,
    query::{Alt, Entities, Modified, Read, With},
    relation::{ChildOf, RelatesExclusive},
    world::World,
};

#[derive(Component)]
struct Name(&'static str);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pose {
    x: f64,
    y: f64,
    angle: f64,
}

impl Pose {
    fn compose(self, local: Self) -> Self {
        let (sin, cos) = self.angle.sin_cos();
        Self {
            x: self.x + cos * local.x - sin * local.y,
            y: self.y + sin * local.x + cos * local.y,
            angle: self.angle + local.angle,
        }
    }
}

#[derive(Component, Clone, Copy)]
struct LocalPose(Pose);
#[derive(Component)]
struct WorldPose(Pose);

fn node(world: &mut World, name: &'static str, pose: Pose, parent: Option<EntityId>) -> EntityId {
    // Dynamic bundles also support runtime choices of components.
    let id = world
        .spawn(
            EntityBuilder::new()
                .with(Name(name))
                .with(LocalPose(pose))
                .with(WorldPose(Pose::default())),
        )
        .id();
    if let Some(parent) = parent {
        world.insert_relation(id, ChildOf, parent).unwrap();
    }
    id
}

fn parent(world: &mut World, id: EntityId) -> Option<EntityId> {
    world
        .get::<Option<RelatesExclusive<With<ChildOf>>>>(id)
        .unwrap()
        .map(|p| p.id())
}

fn propagate(world: &mut World) {
    let locals: BTreeMap<_, _> = world
        .view::<(Entities, &LocalPose)>()
        .into_iter()
        .map(|(id, local)| (id.id(), local.0))
        .collect();
    let parents: BTreeMap<_, _> = locals.keys().map(|&id| (id, parent(world, id))).collect();

    // This authored scene is acyclic. The node-count bound detects invalid
    // ancestry rather than allowing a malformed scene to loop indefinitely.
    for &id in locals.keys() {
        let mut ancestry = Vec::new();
        let mut cursor = Some(id);
        while let Some(ancestor) = cursor {
            assert!(
                ancestry.len() < locals.len(),
                "scene contains a parent cycle"
            );
            ancestry.push(locals[&ancestor]);
            cursor = parents[&ancestor];
        }
        let pose = ancestry
            .into_iter()
            .rev()
            .fold(Pose::default(), Pose::compose);
        let mut stored = world.get::<Alt<WorldPose>>(id).unwrap();
        // Alt marks a component modified only when it is mutably dereferenced.
        if stored.0 != pose {
            stored.0 = pose;
        }
    }
}

fn print_scene(world: &mut World, title: &str) {
    println!("\n{title} (world position; angle in degrees)");
    let nodes: Vec<_> = world
        .view::<(Entities, &Name, &WorldPose)>()
        .into_iter()
        .map(|(id, name, pose)| (id.id(), name.0, pose.0))
        .collect();
    let mut children = BTreeMap::<Option<EntityId>, Vec<_>>::new();
    for (id, name, pose) in nodes {
        children
            .entry(parent(world, id))
            .or_default()
            .push((id, name, pose));
    }
    let mut stack: Vec<_> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|&(id, name, pose)| (id, name, pose, 0))
        .collect();
    let limit = world.view::<&Name>().into_iter().count();
    let mut shown = 0;
    while let Some((id, name, pose, depth)) = stack.pop() {
        assert!(shown < limit && depth < limit, "invalid scene traversal");
        shown += 1;
        println!(
            "{}+- {name:12} ({:5.1}, {:5.1})  {:5.1} deg",
            "  ".repeat(depth),
            pose.x,
            pose.y,
            pose.angle.to_degrees()
        );
        stack.extend(
            children
                .get(&Some(id))
                .into_iter()
                .flatten()
                .rev()
                .map(|&(id, name, pose)| (id, name, pose, depth + 1)),
        );
    }
    assert_eq!(shown, limit, "every node must be reachable from a root");
}

fn assert_position(world: &mut World, id: EntityId, x: f64, y: f64) {
    let pose = world.get::<&WorldPose>(id).unwrap().0;
    assert!(
        (pose.x - x).abs() < 1e-9 && (pose.y - y).abs() < 1e-9,
        "unexpected world pose: {pose:?}"
    );
}

fn main() {
    let mut world = World::new();
    let observatory = node(
        &mut world,
        "Observatory",
        Pose {
            x: 10.0,
            y: 2.0,
            angle: FRAC_PI_2,
        },
        None,
    );
    let arm = node(
        &mut world,
        "Telescope arm",
        Pose {
            x: 3.0,
            ..Pose::default()
        },
        Some(observatory),
    );
    let lens = node(
        &mut world,
        "Lens",
        Pose {
            x: 2.0,
            ..Pose::default()
        },
        Some(arm),
    );
    let beacon = node(
        &mut world,
        "Beacon",
        Pose {
            y: 2.0,
            ..Pose::default()
        },
        Some(observatory),
    );
    let rover = node(
        &mut world,
        "Rover",
        Pose {
            x: -4.0,
            y: 1.0,
            angle: 0.0,
        },
        None,
    );
    propagate(&mut world);
    assert_position(&mut world, lens, 10.0, 7.0);
    assert_position(&mut world, beacon, 8.0, 2.0);
    print_scene(&mut world, "1. The observatory rotates its whole subtree");

    let unchanged = world.epoch();
    propagate(&mut world);
    assert_eq!(
        world
            .view_with(Modified::<Read<WorldPose>>::new(unchanged))
            .into_iter()
            .count(),
        0
    );

    let before_reparent = world.epoch();
    // Reparenting keeps the local pose, so the inherited world pose changes.
    world.insert_relation(arm, ChildOf, rover).unwrap();
    propagate(&mut world);
    assert_position(&mut world, arm, -1.0, 1.0);
    assert_position(&mut world, lens, 1.0, 1.0);
    assert_eq!(
        world
            .view_with(Modified::<Read<WorldPose>>::new(before_reparent))
            .into_iter()
            .count(),
        2
    );
    print_scene(&mut world, "2. The rover carries the telescope");

    world.despawn(observatory).unwrap();
    assert!(!world.is_alive(beacon));
    assert!(world.is_alive(arm) && world.is_alive(lens));
    print_scene(
        &mut world,
        "3. Removing the observatory destroys only its owned subtree",
    );

    world.despawn(rover).unwrap();
    assert!(!world.is_alive(arm) && !world.is_alive(lens));
    assert_eq!(world.view::<Entities>().into_iter().count(), 0);
    println!(
        "\nRover removed: all 5 scene nodes cleaned up. Transform and ownership checks passed."
    );
}
