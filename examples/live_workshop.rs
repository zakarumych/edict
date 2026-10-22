//! A background editor sends commands while the main thread owns the world.
//!
//! Run: cargo run --example live_workshop

use std::{fmt, sync::mpsc, thread};

use edict::{
    action::LocalActionEncoder,
    bundle::EntityBuilder,
    component::Component,
    entity::EntityId,
    epoch::EpochId,
    query::{Alt, Cpy, Entities, Modified, Read},
    relation::ChildOf,
    world::{World, WorldBuilder},
};

#[derive(Component)]
#[edict(borrow(dyn fmt::Display))]
struct Caption(String);
impl fmt::Display for Caption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "name={}", self.0)
    }
}

#[derive(Component)]
#[edict(borrow(dyn fmt::Display))]
struct Mesh(&'static str);
impl fmt::Display for Mesh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mesh={}", self.0)
    }
}

#[derive(Component)]
#[edict(borrow(dyn fmt::Display))]
struct Light(u32);
impl fmt::Display for Light {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "light={} lm", self.0)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct Position(i32, i32);
struct Scene(EntityId);
#[derive(Default)]
struct Catalog(Vec<EntityId>);
#[derive(Default)]
struct Events {
    log: Vec<String>,
    drops: usize,
}

#[derive(Clone, Copy)]
enum Phase {
    Create,
    Edit,
    Delete,
    Close,
}

fn inspect(world: &World, title: &str) {
    println!("\n{title}");
    let mut lines = Vec::new();
    for (entity, values) in world
        .view::<Entities>()
        .borrow_all::<dyn fmt::Display + Sync>()
    {
        let mut fields: Vec<_> = values.map(|value| value.to_string()).collect();
        fields.sort();
        lines.push((entity.id().bits(), fields.join(" | ")));
    }
    lines.sort_unstable();
    for (id, fields) in lines {
        println!("  #{id}: {fields}");
    }
}

fn dirty_positions(world: &World, since: EpochId) -> Vec<EntityId> {
    let mut dirty: Vec<_> = world
        .view::<Entities>()
        .extend(Modified::<Read<Position>>::new(since))
        .into_iter()
        .map(|(entity, position)| {
            println!(
                "  dirty #{}: position=({}, {})",
                entity.id().bits(),
                position.0,
                position.1
            );
            entity.id()
        })
        .collect();
    dirty.sort_unstable();
    dirty
}

fn main() {
    let mut builder = WorldBuilder::new();
    builder.register_component::<Caption>().on_drop_fn(
        |caption: &mut Caption, _: EntityId, mut actions: LocalActionEncoder<'_>| {
            let name = caption.0.clone();
            actions.closure(move |world| {
                let mut events = world.expect_resource_mut::<Events>();
                events.drops += 1;
                events.log.push(format!("drop hook: {name}"));
            });
        },
    );
    let mut world = builder.build();
    let scene = world.spawn_empty().id();
    world.insert_resource(Scene(scene));
    world.insert_resource(Catalog::default());
    world.insert_resource(Events::default());

    // Only the sender crosses threads. Acknowledgments mean commands are queued;
    // the main thread chooses the boundary at which they become world mutations.
    let sender = world.new_action_sender();
    let (requests, phases) = mpsc::channel();
    let (acknowledgments, ready) = mpsc::channel();
    let producer = thread::spawn(move || {
        while let Ok(phase) = phases.recv() {
            match phase {
                Phase::Create => sender.closure(|world| {
                    let parent = world.expect_resource::<Scene>().0;
                    for (index, name) in ["Workbench", "Studio lamp", "Preview camera"]
                        .into_iter()
                        .enumerate()
                    {
                        let mut entity = EntityBuilder::new()
                            .with(Caption(name.into()))
                            .with(Position(index as i32 * 10, 0));
                        if index == 1 {
                            entity.add(Light(800));
                        } else {
                            entity.add(Mesh(if index == 0 {
                                "bench.glb"
                            } else {
                                "camera.glb"
                            }));
                        }
                        let id = world.spawn(entity).id();
                        world.insert_relation(id, ChildOf, parent).unwrap();
                        world.expect_resource_mut::<Catalog>().0.push(id);
                    }
                    world
                        .expect_resource_mut::<Events>()
                        .log
                        .push("create: 3 dynamic entities".into());
                }),
                Phase::Edit => sender.closure(|world| {
                    let lamp = world.expect_resource::<Catalog>().0[1];
                    // Alt lets an inspector visit all positions while touching only its selection.
                    for (entity, mut position) in world.view::<(Entities, Alt<Position>)>() {
                        if entity.id() == lamp {
                            position.0 += 5;
                            position.1 = 3;
                        }
                    }
                    world.get::<&mut Light>(lamp).unwrap().0 = 1200;
                    world
                        .expect_resource_mut::<Events>()
                        .log
                        .push("edit: lamp moved and brightened".into());
                }),
                Phase::Delete => sender.closure(|world| {
                    let camera = world.expect_resource::<Catalog>().0[2];
                    world.despawn(camera).unwrap();
                    world
                        .expect_resource_mut::<Catalog>()
                        .0
                        .retain(|&id| id != camera);
                    world
                        .expect_resource_mut::<Events>()
                        .log
                        .push("delete: preview camera".into());
                }),
                Phase::Close => sender.despawn(scene),
            }
            acknowledgments.send(()).unwrap();
        }
    });

    requests.send(Phase::Create).unwrap();
    ready.recv().unwrap();
    assert!(world.expect_resource::<Catalog>().0.is_empty());
    world.execute_received_actions();
    let ids = world.expect_resource::<Catalog>().0.clone();
    assert_eq!(ids.len(), 3);
    inspect(&world, "created: heterogeneous trait-object inspector");

    let cutoff = world.epoch();
    requests.send(Phase::Edit).unwrap();
    ready.recv().unwrap();
    assert_eq!(world.get::<Cpy<Position>>(ids[1]).unwrap(), Position(10, 0));
    world.execute_received_actions();
    inspect(&world, "edited: queued changes applied at frame boundary");
    assert_eq!(world.get::<Cpy<Position>>(ids[1]).unwrap(), Position(15, 3));
    println!("\nposition change inspector:");
    assert_eq!(dirty_positions(&world, cutoff), [ids[1]]);
    assert_eq!(world.get::<Cpy<Position>>(ids[0]).unwrap(), Position(0, 0));

    requests.send(Phase::Delete).unwrap();
    ready.recv().unwrap();
    assert!(world.is_alive(ids[2]));
    world.execute_received_actions();
    assert!(!world.is_alive(ids[2]));
    assert_eq!(world.expect_resource::<Events>().drops, 1);
    inspect(&world, "deleted: camera removed, scene still live");

    requests.send(Phase::Close).unwrap();
    ready.recv().unwrap();
    assert!(world.is_alive(scene));
    world.execute_received_actions();
    assert!(!world.is_alive(scene));
    assert!(ids.iter().all(|&id| !world.is_alive(id)));
    assert_eq!(world.expect_resource::<Events>().drops, 3);
    drop(requests);
    producer.join().unwrap();

    println!("\nevent journal:");
    for event in &world.expect_resource::<Events>().log {
        println!("  {event}");
    }
    println!("scene closed: 3 drop hooks, zero live entities, producer joined");
    assert_eq!(world.view::<Entities>().into_iter().count(), 0);
}
