//! A deterministic fountain rendered directly from ECS state as ASCII frames.
//! Run with `cargo run --example particle_fountain`.

use edict::{
    action::ActionBuffer,
    component::Component,
    query::{Alt, Entities, Modified, Read},
    world::World,
};

const WIDTH: usize = 61;
const HEIGHT: usize = 17;
const EMISSION_TICKS: usize = 48;
const PER_TICK: usize = 32;
const FINAL_TICK: usize = 120;

#[derive(Component, Clone, Copy)]
struct Position {
    x: i32,
    y: i32,
}
#[derive(Component)]
struct Velocity {
    x: i32,
    y: i32,
}
#[derive(Component)]
struct Remaining(u16);
#[derive(Component)]
struct Serial(usize);

#[derive(Default)]
struct Statistics {
    emitted: usize,
    retired: usize,
    peak_alive: usize,
}

// Integer integration and a fixed seed make frames reproducible.
struct Random(u64);
impl Random {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
}

fn emit(world: &mut World, random: &mut Random) {
    let first = world.expect_resource::<Statistics>().emitted;
    // Consuming the spawning iterator materializes every bundle in this burst.
    world
        .spawn_batch((0..PER_TICK).map(|offset| {
            let vx = (random.next() % 19) as i32 - 9;
            let vy = 15 + (random.next() % 12) as i32;
            let life = 35 + (random.next() % 30) as u16;
            (
                Position { x: 0, y: 0 },
                Velocity { x: vx, y: vy },
                Remaining(life),
                Serial(first + offset),
            )
        }))
        .spawn_all();
    world.expect_resource_mut::<Statistics>().emitted += PER_TICK;
}

fn simulate(world: &mut World, retirements: &mut ActionBuffer) {
    let before_step = world.epoch();
    let alive_before = world.view::<&Position>().into_iter().count();
    let expired;
    {
        let mut encoder = retirements.encoder(world);
        let mut queued = 0;
        for (id, mut position, velocity, remaining) in
            world.view::<(Entities, Alt<Position>, &mut Velocity, &mut Remaining)>()
        {
            assert!(remaining.0 > 0);
            position.x += velocity.x;
            position.y += velocity.y;
            velocity.y -= 1;
            remaining.0 -= 1;
            if remaining.0 == 0 || position.y < 0 {
                encoder.despawn(id);
                queued += 1;
            }
        }
        expired = queued;
    }
    // Retirement is deferred until iteration releases its component borrows.
    assert_eq!(world.view::<&Position>().into_iter().count(), alive_before);
    assert_eq!(
        world
            .view_with(Modified::<Read<Position>>::new(before_step))
            .into_iter()
            .count(),
        alive_before
    );
    retirements.execute(world);
    let alive_after = world.view::<&Position>().into_iter().count();
    assert_eq!(alive_after + expired, alive_before);
    assert_eq!(
        world
            .view_with(Modified::<Read<Position>>::new(before_step))
            .into_iter()
            .count(),
        alive_after
    );
    let mut stats = world.expect_resource_mut::<Statistics>();
    stats.retired += expired;
    stats.peak_alive = stats.peak_alive.max(alive_after);
    assert_eq!(stats.emitted, stats.retired + alive_after);
}

fn draw(world: &World, tick: usize) {
    let mut density = [[0u16; WIDTH]; HEIGHT];
    let mut outside = 0;
    for position in world.view::<&Position>() {
        let column = WIDTH as i32 / 2 + position.x / 14;
        let row = HEIGHT as i32 - 2 - position.y / 24;
        if (0..WIDTH as i32).contains(&column) && (0..HEIGHT as i32 - 1).contains(&row) {
            density[row as usize][column as usize] += 1;
        } else {
            outside += 1;
        }
    }
    let stats = world.expect_resource::<Statistics>();
    println!(
        "\nTick {tick:3} | alive {:4} | retired {:4} | outside frame {outside}",
        stats.emitted - stats.retired,
        stats.retired
    );
    println!("+{}+", "-".repeat(WIDTH));
    for row in density.iter().take(HEIGHT - 1) {
        let line: String = row
            .iter()
            .map(|count| match count {
                0 => ' ',
                1 => '.',
                2..=4 => '+',
                5..=12 => '*',
                _ => '#',
            })
            .collect();
        println!("|{line}|");
    }
    println!("|{}^{}|", "_".repeat(WIDTH / 2), "_".repeat(WIDTH / 2));
    println!("+{}+", "-".repeat(WIDTH));
}

fn main() {
    let mut world = World::new();
    world.insert_resource(Statistics::default());
    let mut random = Random(0x5eed_cafe);
    let mut retirements = ActionBuffer::new();
    println!("PARTICLE FOUNTAIN | density: . = 1, + = 2-4, * = 5-12, # = 13+; ^ emitter");
    println!("48 bursts x 32 particles; integer gravity; queued despawns after each step.");
    draw(&world, 0);
    for tick in 1..=FINAL_TICK {
        if tick <= EMISSION_TICKS {
            emit(&mut world, &mut random);
        }
        simulate(&mut world, &mut retirements);
        for (position, remaining, serial) in world.view::<(&Position, &Remaining, &Serial)>() {
            assert!(position.y >= 0 && remaining.0 > 0);
            assert!(serial.0 < world.expect_resource::<Statistics>().emitted);
        }
        if [32, 64, FINAL_TICK].contains(&tick) {
            draw(&world, tick);
        }
    }
    {
        let stats = world.expect_resource::<Statistics>();
        assert_eq!(stats.emitted, EMISSION_TICKS * PER_TICK);
        assert_eq!(stats.retired, stats.emitted);
        assert!(stats.peak_alive > 1000);
    }
    assert_eq!(world.view::<Entities>().into_iter().count(), 0);
    assert!(!retirements.execute(&mut world));
    let stats = world.expect_resource::<Statistics>();
    println!(
        "\nComplete: {} emitted, {} retired, peak {} alive. No particles or pending retirements remain.",
        stats.emitted, stats.retired, stats.peak_alive
    );
}
