//! Binary checkpoints with explicit application state and deterministic replay.
//!
//! Run: cargo run --example checkpoint_replay --features nanoserde

use edict::{
    action::ActionBuffer,
    component::Component,
    dump::{WorldDump, WorldLoad},
    entity::EntityId,
    epoch::EpochId,
    query::{Alt, Cpy, Entities, Modified, Read, With},
    relation::ChildOf,
    world::World,
};
use nanoserde::{DeBin, DeBinErr, SerBin};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct Position(i32);
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct Velocity(i32);
#[derive(Component)]
struct SavedActor;
#[derive(Clone, Copy)]
struct Simulation {
    frame: u32,
}

// Primitive codecs keep the example independent of optional derive features.
macro_rules! binary_component {
    ($ty:ident) => {
        impl SerBin for $ty {
            fn ser_bin(&self, output: &mut Vec<u8>) {
                self.0.ser_bin(output);
            }
        }
        impl DeBin for $ty {
            fn de_bin(offset: &mut usize, input: &[u8]) -> Result<Self, DeBinErr> {
                i32::de_bin(offset, input).map(Self)
            }
        }
    };
}
binary_component!(Position);
binary_component!(Velocity);

struct Checkpoint {
    components: Vec<u8>,
    simulation: Simulation,
}

fn checkpoint(world: &World, after: EpochId) -> Checkpoint {
    Checkpoint {
        components: WorldDump::<(Position, Velocity), _>::new(world, With::<SavedActor>, after)
            .serialize_bin(),
        // Resources are application checkpoint state, separate from component dumps.
        simulation: *world.expect_resource::<Simulation>(),
    }
}

fn restore(world: &mut World, group: EntityId, checkpoint: &Checkpoint) {
    let mut buffer = ActionBuffer::new();
    let marker = move |world: &mut World, id: EntityId| {
        world.with(id, || SavedActor).unwrap();
        // Relations are rebuilt explicitly; they are not part of this dump set.
        world.insert_relation(id, ChildOf, group).unwrap();
    };
    WorldLoad::<(Position, Velocity), _>::new(world, marker)
        .load_bin(&mut buffer.encoder(world), &checkpoint.components)
        .unwrap();
    buffer.execute(world);
    world.insert_resource(checkpoint.simulation);
}

fn tick(world: &mut World) {
    for (position, velocity) in world.view::<(&mut Position, &Velocity)>() {
        position.0 += velocity.0;
    }
    world.expect_resource_mut::<Simulation>().frame += 1;
}

fn state(world: &World) -> Vec<(u64, i32, Option<i32>)> {
    let mut rows: Vec<_> = world
        .view::<(Entities, &Position, Option<&Velocity>)>()
        .into_iter()
        .map(|(entity, position, velocity)| {
            (
                entity.id().bits(),
                position.0,
                velocity.map(|value| value.0),
            )
        })
        .collect();
    rows.sort_unstable();
    rows
}

fn checksum(rows: &[(u64, i32, Option<i32>)]) -> i64 {
    rows.iter()
        .map(|&(id, position, velocity)| {
            id as i64 * 31 + i64::from(position) * 17 + i64::from(velocity.unwrap_or(0))
        })
        .sum()
}

fn main() {
    let mut original = World::new();
    original.insert_resource(Simulation { frame: 0 });
    let ids: Vec<_> = original
        .spawn_batch(
            (0..32).map(|index| (Position(index * 100), Velocity(index % 7 + 1), SavedActor)),
        )
        .map(|entity| entity.id())
        .collect();
    for _ in 0..2 {
        tick(&mut original);
    }
    let full = checkpoint(&original, EpochId::start());
    let cutoff = original.epoch();

    // The host-owned group uses an ID outside the source actor range.
    let group = EntityId::from_bits(1 << 60).unwrap();
    let mut resumed = World::new();
    resumed.spawn_at(group, ());
    restore(&mut resumed, group, &full);
    assert_eq!(state(&original), state(&resumed));
    println!(
        "full checkpoint: {} actors, {} bytes, frame {}",
        ids.len(),
        full.components.len(),
        full.simulation.frame
    );

    for _ in 0..3 {
        tick(&mut original);
    }
    for (entity, mut velocity) in original.view::<(Entities, Alt<Velocity>)>() {
        if entity.id() == ids[2] {
            velocity.0 *= 2;
        }
    }
    let mut edits = ActionBuffer::new();
    edits.encoder(&original).drop::<Velocity>(ids[5]);
    edits.execute(&mut original);

    let dirty_positions = original
        .view_with(Modified::<Read<Position>>::new(cutoff))
        .into_iter()
        .count();
    let dirty_velocities = original
        .view_with(Modified::<Read<Velocity>>::new(cutoff))
        .into_iter()
        .count();
    assert_eq!((dirty_positions, dirty_velocities), (32, 1));
    let delta = checkpoint(&original, cutoff);
    restore(&mut resumed, group, &delta);
    assert_eq!(state(&original), state(&resumed));
    assert_eq!(resumed.get::<Option<&Velocity>>(ids[5]).unwrap(), None);
    println!(
        "delta checkpoint: {} bytes; {} positions + {} velocity payloads, frame {}",
        delta.components.len(),
        dirty_positions,
        dirty_velocities,
        delta.simulation.frame
    );
    assert!(delta.components.len() < full.components.len());

    // Both simulations continue from the same checkpoint and independent resources.
    for _ in 0..2 {
        tick(&mut original);
        tick(&mut resumed);
    }
    for (index, &id) in ids.iter().enumerate() {
        let speed = index as i32 % 7 + 1;
        let total_steps = if index == 5 {
            5
        } else if index == 2 {
            9
        } else {
            7
        };
        let expected = Position(index as i32 * 100 + total_steps * speed);
        assert_eq!(original.get::<Cpy<Position>>(id).unwrap(), expected);
        assert_eq!(resumed.get::<Cpy<Position>>(id).unwrap(), expected);
    }
    let original_state = state(&original);
    let resumed_state = state(&resumed);
    assert_eq!(original_state, resumed_state);
    assert_eq!(original.expect_resource::<Simulation>().frame, 7);
    assert_eq!(resumed.expect_resource::<Simulation>().frame, 7);
    println!(
        "replay verified: frame 7, checksum {}, all {} entity IDs match",
        checksum(&resumed_state),
        resumed_state.len()
    );

    resumed.despawn(group).unwrap();
    assert!(ids.iter().all(|&id| !resumed.is_alive(id)));
    assert_eq!(original.view::<&Position>().into_iter().count(), 32);
    println!("owned-group cleanup: 32 restored actors removed; original simulation intact");
}
