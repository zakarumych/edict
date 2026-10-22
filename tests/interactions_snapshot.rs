use std::{collections::VecDeque, convert::Infallible};

use edict::{
    action::{ActionBuffer, LocalActionEncoder},
    component::Component,
    dump::{DumpSet, DumpSlot, Dumper, EntityDump, LoadSet, LoadSlot, Loader, Mark},
    entity::EntityId,
    epoch::EpochId,
    query::{Alt, Cpy, Entities, With},
    relation::ChildOf,
    world::World,
};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "alkahest",
    derive(
        alkahest_proc::Formula,
        alkahest_proc::SerializeRef,
        alkahest_proc::Deserialize
    )
)]
struct Position(i32);
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "alkahest",
    derive(
        alkahest_proc::Formula,
        alkahest_proc::SerializeRef,
        alkahest_proc::Deserialize
    )
)]
struct Velocity(i32);
#[derive(Component)]
struct Persisted;
#[derive(Component)]
struct Temporary;

#[derive(Default)]
struct Audit {
    marked: usize,
    dropped: usize,
}
#[derive(Clone, Copy)]
struct Clock(u32);

struct Restored;
impl Component for Restored {
    fn on_drop(&mut self, _: EntityId, mut actions: LocalActionEncoder<'_>) {
        actions.closure(|world| world.expect_resource_mut::<Audit>().dropped += 1);
    }
}

fn marker(parent: EntityId) -> impl Mark {
    move |world: &mut World, id: EntityId| {
        world.with(id, || Persisted).unwrap();
        world.with(id, || Restored).unwrap();
        world.insert_relation(id, ChildOf, parent).unwrap();
        world.expect_resource_mut::<Audit>().marked += 1;
    }
}

#[derive(Clone)]
struct Row {
    header: [u64; 3],
    position: Option<Position>,
    velocity: Option<Velocity>,
}
#[derive(Default)]
struct Memory(Vec<Row>);
impl Dumper<(Position, Velocity)> for Memory {
    type Error = Infallible;
    fn dump(
        &mut self,
        entity: EntityDump,
        (position, velocity): (DumpSlot<'_, Position>, DumpSlot<'_, Velocity>),
    ) -> Result<(), Self::Error> {
        self.0.push(Row {
            header: entity.0,
            position: match position {
                DumpSlot::Skipped => None,
                DumpSlot::Component(value) => Some(*value),
            },
            velocity: match velocity {
                DumpSlot::Skipped => None,
                DumpSlot::Component(value) => Some(*value),
            },
        });
        Ok(())
    }
}

struct Reader {
    rows: VecDeque<Row>,
    current: Option<Row>,
}
impl Loader<(Position, Velocity)> for Reader {
    type Error = Infallible;
    fn next(&mut self) -> Result<Option<EntityDump>, Self::Error> {
        self.current = self.rows.pop_front();
        Ok(self.current.as_ref().map(|row| EntityDump(row.header)))
    }
    fn load(
        &mut self,
        (position, velocity): &mut (LoadSlot<'_, Position>, LoadSlot<'_, Velocity>),
    ) -> Result<(), Self::Error> {
        let row = self.current.as_ref().unwrap();
        match position {
            LoadSlot::Skipped => {}
            LoadSlot::Missing => *position = LoadSlot::Created(row.position.unwrap()),
            LoadSlot::Existing(value) => **value = row.position.unwrap(),
            LoadSlot::Created(_) => unreachable!(),
        }
        match velocity {
            LoadSlot::Skipped => {}
            LoadSlot::Missing => *velocity = LoadSlot::Created(row.velocity.unwrap()),
            LoadSlot::Existing(value) => **value = row.velocity.unwrap(),
            LoadSlot::Created(_) => unreachable!(),
        }
        Ok(())
    }
}

fn encode_memory(world: &World, epoch: EpochId) -> Memory {
    let mut memory = Memory::default();
    <(Position, Velocity)>::dump_world(world, With::<Persisted>, epoch, &mut memory).unwrap();
    memory
}

fn queue_memory(world: &World, parent: EntityId, memory: &Memory, actions: &mut ActionBuffer) {
    let mut reader = Reader {
        rows: memory.0.clone().into(),
        current: None,
    };
    <(Position, Velocity)>::load_world(
        world,
        marker(parent),
        &mut actions.encoder(world),
        &mut reader,
    )
    .unwrap();
}

fn target_world() -> (World, EntityId) {
    let mut world = World::new();
    let parent = world.spawn_empty().id();
    world.insert_resource(Audit::default());
    world.insert_resource(Clock(0));
    (world, parent)
}

#[cfg(feature = "scheduler")]
fn advance(world: &mut World) {
    fn simulate(
        mut actors: edict::view::View<(&mut Position, &Velocity)>,
        mut clock: edict::resources::ResMut<Clock>,
    ) {
        for (position, velocity) in actors.iter_mut() {
            position.0 += velocity.0;
        }
        clock.0 += 1;
    }
    let mut scheduler = edict::scheduler::Scheduler::new();
    scheduler.add_system(simulate);
    scheduler.run_sequential(world);
}

#[cfg(not(feature = "scheduler"))]
fn advance(world: &mut World) {
    for (position, velocity) in world.view::<(&mut Position, &Velocity)>() {
        position.0 += velocity.0;
    }
    world.expect_resource_mut::<Clock>().0 += 1;
}

fn exercise_resumed_simulation<S>(
    encode: impl Fn(&World, EpochId) -> S,
    queue: impl Fn(&World, EntityId, &S, &mut ActionBuffer),
) {
    let mut source = World::new();
    let source_parent = source.spawn_empty().id();
    source.insert_resource(Clock(0));
    let ids: Vec<_> = source
        .spawn_batch((0..600).map(|i| (Position(i), Velocity(2), Persisted)))
        .map(|e| e.id())
        .collect();
    let mut initial = ActionBuffer::new();
    initial.encoder(&source).drop::<Velocity>(ids[3]);
    initial.execute(&mut source);
    for &id in &ids {
        source.insert_relation(id, ChildOf, source_parent).unwrap();
    }
    let full = encode(&source, EpochId::start());
    let (mut restored, parent) = target_world();
    assert_eq!(parent, source_parent);
    let mut actions = ActionBuffer::new();
    queue(&restored, parent, &full, &mut actions);
    assert!(!restored.is_alive(ids[0]));
    assert_eq!(restored.expect_resource::<Audit>().marked, 0);
    actions.execute(&mut restored);
    assert_eq!(restored.view::<&Position>().into_iter().count(), 600);
    assert_eq!(
        restored
            .view::<Entities>()
            .filter_relates_to::<ChildOf>(parent)
            .into_iter()
            .count(),
        600
    );
    assert_eq!(restored.get::<Option<&Velocity>>(ids[3]).unwrap(), None);
    let cutoff = source.epoch();
    {
        let mut view = source.view::<&mut Position>();
        for batch in view.iter_batched_mut(130) {
            for position in batch {
                position.0 += 10;
            }
        }
    }
    for (position, mut velocity) in source.view::<(&Position, Alt<Velocity>)>() {
        if position.0 % 2 == 0 {
            velocity.0 += 1;
        }
    }
    let mut commands = ActionBuffer::new();
    {
        let mut encoder = commands.encoder(&source);
        encoder.drop::<Velocity>(ids[1]);
        encoder.insert(ids[3], Velocity(7));
        encoder.insert(ids[4], Temporary);
        encoder.closure(|world| world.expect_resource_mut::<Clock>().0 = 4);
    }
    commands.execute(&mut source);
    source.drop::<Temporary>(ids[4]).unwrap();
    let delta = encode(&source, cutoff);
    queue(&restored, parent, &delta, &mut actions);
    actions.execute(&mut restored);
    // Resources belong to the application's checkpoint alongside the component dump.
    restored.insert_resource(*source.expect_resource::<Clock>());
    assert_eq!(restored.expect_resource::<Audit>().marked, 1200);
    assert_eq!(restored.expect_resource::<Audit>().dropped, 0);
    for _ in 0..2 {
        advance(&mut source);
        advance(&mut restored);
    }
    for (index, &id) in ids.iter().enumerate() {
        let speed = if index == 1 {
            None
        } else if index == 3 {
            Some(7)
        } else if index % 2 == 0 {
            Some(3)
        } else {
            Some(2)
        };
        let expected = Position(index as i32 + 10 + 2 * speed.unwrap_or(0));
        assert_eq!(restored.get::<Cpy<Position>>(id).unwrap(), expected);
        assert_eq!(source.get::<Cpy<Position>>(id).unwrap(), expected);
        assert_eq!(
            restored.get::<Option<&Velocity>>(id).unwrap().copied(),
            speed.map(Velocity)
        );
    }
    assert_eq!(restored.expect_resource::<Clock>().0, 6);
    restored.despawn(parent).unwrap();
    assert!(ids.iter().all(|&id| !restored.is_alive(id)));
    assert_eq!(restored.expect_resource::<Audit>().dropped, 600);
    assert_eq!(source.view::<&Position>().into_iter().count(), 600);
}

#[test]
fn memory_checkpoint_delta_and_resumed_simulation_preserve_actor_state() {
    exercise_resumed_simulation(encode_memory, queue_memory);
}

#[test]
fn checkpoint_applies_explicit_removals_for_actors_without_dump_components() {
    let mut source = World::new();
    source.spawn_empty();
    let actor = source.spawn((Position(12), Velocity(3), Persisted)).id();
    let (mut restored, parent) = target_world();
    let mut actions = ActionBuffer::new();
    queue_memory(
        &restored,
        parent,
        &encode_memory(&source, EpochId::start()),
        &mut actions,
    );
    actions.execute(&mut restored);
    let cutoff = source.epoch();
    let mut commands = ActionBuffer::new();
    {
        let mut encoder = commands.encoder(&source);
        encoder.drop::<Position>(actor);
        encoder.drop::<Velocity>(actor);
    }
    commands.execute(&mut source);
    assert_eq!(
        restored
            .get::<(Cpy<Position>, Cpy<Velocity>)>(actor)
            .unwrap(),
        (Position(12), Velocity(3))
    );
    let delta = encode_memory(&source, cutoff);
    // Actors without any dumped component require application-side removal records.
    assert!(delta.0.is_empty());
    queue_memory(&restored, parent, &delta, &mut actions);
    {
        let mut encoder = actions.encoder(&restored);
        encoder.drop::<Position>(actor);
        encoder.drop::<Velocity>(actor);
    }
    assert_eq!(
        restored
            .get::<(Cpy<Position>, Cpy<Velocity>)>(actor)
            .unwrap(),
        (Position(12), Velocity(3))
    );
    actions.execute(&mut restored);
    assert!(restored.is_alive(actor));
    assert_eq!(
        restored
            .get::<(Option<&Position>, Option<&Velocity>)>(actor)
            .unwrap(),
        (None, None)
    );
    assert_eq!(
        restored
            .view::<Entities>()
            .filter_relates_to::<ChildOf>(parent)
            .into_iter()
            .count(),
        1
    );
    assert_eq!(restored.expect_resource::<Audit>().dropped, 0);
    restored.despawn(parent).unwrap();
    assert!(!restored.is_alive(actor));
    assert_eq!(restored.expect_resource::<Audit>().dropped, 1);
}

#[test]
fn filtered_checkpoint_rebuilds_only_selected_actors_and_hook_driven_cleanup() {
    let mut source = World::new();
    source.spawn_empty();
    let selected = source.spawn((Position(10), Persisted)).id();
    let ignored = source.spawn((Position(20), Velocity(5))).id();
    let snapshot = encode_memory(&source, EpochId::start());
    let (mut restored, parent) = target_world();
    let mut commands = ActionBuffer::new();
    queue_memory(&restored, parent, &snapshot, &mut commands);
    commands.encoder(&restored).closure(move |world| {
        assert_eq!(
            world
                .view::<Entities>()
                .filter_relates_to::<ChildOf>(parent)
                .into_iter()
                .count(),
            1
        );
        world.insert_resource(Clock(7));
    });
    assert_eq!(restored.expect_resource::<Clock>().0, 0);
    commands.execute(&mut restored);
    assert_eq!(
        restored.get::<Cpy<Position>>(selected).unwrap(),
        Position(10)
    );
    assert!(!restored.is_alive(ignored));
    advance(&mut restored);
    assert_eq!(
        restored.get::<Cpy<Position>>(selected).unwrap(),
        Position(10)
    );
    assert_eq!(restored.expect_resource::<Clock>().0, 8);
    commands.encoder(&restored).despawn(parent);
    commands.execute(&mut restored);
    assert_eq!(restored.expect_resource::<Audit>().dropped, 1);
    assert!(source.is_alive(selected));
    assert!(source.is_alive(ignored));
}

#[cfg(feature = "nanoserde")]
mod nanoserde_backend {
    use super::*;
    use nanoserde::{DeBin, DeBinErr, SerBin};

    macro_rules! binary {
        ($ty:ident) => {
            impl SerBin for $ty {
                fn ser_bin(&self, output: &mut Vec<u8>) {
                    self.0.ser_bin(output);
                }
            }
            impl DeBin for $ty {
                fn de_bin(offset: &mut usize, data: &[u8]) -> Result<Self, DeBinErr> {
                    i32::de_bin(offset, data).map(Self)
                }
            }
        };
    }
    binary!(Position);
    binary!(Velocity);

    #[test]
    fn binary_checkpoint_delta_and_resumed_simulation_preserve_actor_state() {
        exercise_resumed_simulation(
            |world, epoch| {
                edict::dump::WorldDump::<(Position, Velocity), _>::new(
                    world,
                    With::<Persisted>,
                    epoch,
                )
                .serialize_bin()
            },
            |world, parent, data, actions| {
                edict::dump::WorldLoad::<(Position, Velocity), _>::new(world, marker(parent))
                    .load_bin(&mut actions.encoder(world), data)
                    .unwrap();
            },
        );
    }
}

#[cfg(feature = "alkahest")]
#[test]
fn alkahest_checkpoint_delta_and_resumed_simulation_preserve_actor_state() {
    exercise_resumed_simulation(
        |world, epoch| {
            let mut output = Vec::new();
            let (size, root) = edict::dump::WorldDump::<(Position, Velocity), _>::new(
                world,
                With::<Persisted>,
                epoch,
            )
            .dump_alkahest::<Position, Velocity>(&mut output);
            output.truncate(size);
            (output, root)
        },
        |world, parent, (data, root), actions| {
            edict::dump::WorldLoad::<(Position, Velocity), _>::new(world, marker(parent))
                .load_alkahest::<Position, Velocity>(&mut actions.encoder(world), data, *root)
                .unwrap();
        },
    );
}
