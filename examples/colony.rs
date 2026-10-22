//! A colony turns power and labor into ore, metal, and new infrastructure.
//!
//! Run with `cargo run --example colony`. Each schedule is one simulation tick;
//! construction actions become visible when that tick finishes.

use edict::{
    action::{ActionEncoder, LocalActionEncoder},
    component::Component,
    entity::EntityId,
    query::{Entities, Modified},
    relation::ChildOf,
    resources::{Res, ResMut},
    scheduler::Scheduler,
    system::State,
    view::View,
    world::World,
};

#[derive(Component)]
struct Name(&'static str);
#[derive(Component)]
struct Generator(u32);
#[derive(Component)]
struct Miner;
#[derive(Component)]
struct Smelter;
#[derive(Component)]
struct Output(u32);

// Despawning the settlement reclaims every owned asset through component hooks.
struct Salvage {
    name: &'static str,
    metal: u32,
}
impl Component for Salvage {
    fn on_drop(&mut self, _: EntityId, mut actions: LocalActionEncoder) {
        let (name, metal) = (self.name, self.metal);
        actions.closure(move |world| {
            let mut stock = world.expect_resource_mut::<Stock>();
            stock.metal += metal;
            stock.reclaimed += metal;
            drop(stock);
            world.expect_resource_mut::<Cleanup>().0.push(name);
        });
    }
}
#[derive(Default)]
struct Stock {
    ore: u32,
    metal: u32,
    mined: u32,
    smelted: u32,
    invested: u32,
    reclaimed: u32,
}
#[derive(Default)]
struct Grid {
    available: u32,
    generated: u32,
}
#[derive(Default)]
struct Tick(u32);
struct Settlement(EntityId);
#[derive(Default)]
struct Cleanup(Vec<&'static str>);
#[derive(Default)]
struct Telemetry {
    changed: Vec<&'static str>,
    construction: Vec<&'static str>,
}

fn clock(mut tick: ResMut<Tick>, mut elapsed: State<u32>) {
    *elapsed += 1;
    tick.0 = *elapsed;
}
fn generate(generators: View<&Generator>, mut grid: ResMut<Grid>) {
    grid.generated = generators.into_iter().map(|generator| generator.0).sum();
    grid.available = grid.generated;
}
fn mine(miners: View<(&Miner, &mut Output)>, mut grid: ResMut<Grid>, mut stock: ResMut<Stock>) {
    for (_, output) in miners {
        if grid.available == 0 {
            break;
        }
        grid.available -= 1;
        stock.ore += 2;
        stock.mined += 2;
        output.0 += 2;
    }
}
fn smelt(
    smelters: View<(&Smelter, &mut Output)>,
    mut grid: ResMut<Grid>,
    mut stock: ResMut<Stock>,
) {
    for (_, output) in smelters {
        if grid.available < 2 || stock.ore < 3 {
            break;
        }
        grid.available -= 2;
        stock.ore -= 3;
        stock.metal += 1;
        stock.smelted += 1;
        output.0 += 1;
    }
}
fn construct(
    tick: Res<Tick>,
    settlement: Res<Settlement>,
    mut stock: ResMut<Stock>,
    mut telemetry: ResMut<Telemetry>,
    mut actions: ActionEncoder,
) {
    telemetry.construction.clear();
    if tick.0 == 2 && stock.metal >= 2 {
        stock.metal -= 2;
        stock.invested += 2;
        let generator = actions
            .spawn((
                Name("solar annex"),
                Generator(4),
                Salvage {
                    name: "solar annex",
                    metal: 2,
                },
            ))
            .id();
        let miner = actions
            .spawn((
                Name("miner C"),
                Miner,
                Output(0),
                Salvage {
                    name: "miner C",
                    metal: 0,
                },
            ))
            .id();
        actions.insert_relation(generator, ChildOf, settlement.0);
        actions.insert_relation(miner, ChildOf, settlement.0);
        telemetry.construction.push("solar annex + miner C");
    }
    if tick.0 == 4 && stock.metal >= 2 {
        stock.metal -= 2;
        stock.invested += 2;
        let smelter = actions
            .spawn((
                Name("smelter B"),
                Smelter,
                Output(0),
                Salvage {
                    name: "smelter B",
                    metal: 2,
                },
            ))
            .id();
        actions.insert_relation(smelter, ChildOf, settlement.0);
        telemetry.construction.push("smelter B");
    }
}
// The query owns its epoch cutoff: no manual bookkeeping between schedule runs.
fn telemetry(changed: View<(&Name, Modified<&Output>)>, mut telemetry: ResMut<Telemetry>) {
    telemetry.changed = changed.into_iter().map(|(name, _)| name.0).collect();
    telemetry.changed.sort();
}
fn assert_conservation(stock: &Stock) {
    assert_eq!(stock.mined, stock.ore + stock.smelted * 3);
    assert_eq!(
        stock.smelted + stock.reclaimed,
        stock.metal + stock.invested
    );
}

fn main() {
    let mut world = World::new();
    let settlement = world.spawn(()).id();
    world.insert_resource(Settlement(settlement));
    world.insert_resource(Stock::default());
    world.insert_resource(Grid::default());
    world.insert_resource(Tick::default());
    world.insert_resource(Cleanup::default());
    world.insert_resource(Telemetry::default());
    let solar = world
        .spawn((
            Name("solar field"),
            Generator(6),
            Salvage {
                name: "solar field",
                metal: 0,
            },
        ))
        .id();
    world.insert_relation(solar, ChildOf, settlement).unwrap();
    for name in ["miner A", "miner B"] {
        let worker = world
            .spawn((Name(name), Miner, Output(0), Salvage { name, metal: 0 }))
            .id();
        world.insert_relation(worker, ChildOf, settlement).unwrap();
    }
    let smelter = world
        .spawn((
            Name("smelter A"),
            Smelter,
            Output(0),
            Salvage {
                name: "smelter A",
                metal: 0,
            },
        ))
        .id();
    world.insert_relation(smelter, ChildOf, settlement).unwrap();
    let mut schedule = Scheduler::new();
    schedule.add_system(clock);
    schedule.add_system(generate);
    schedule.add_system(mine);
    schedule.add_system(smelt);
    schedule.add_system(construct);
    schedule.add_system(telemetry);

    println!("COLONY / eight deterministic production ticks");
    println!("tick | power used/total | ore | metal | miners | furnaces | changed | construction");
    for _ in 0..8 {
        schedule.run_sequential(&mut world);
        let stock = world.expect_resource::<Stock>();
        let grid = world.expect_resource::<Grid>();
        let report = world.expect_resource::<Telemetry>();
        assert_conservation(&stock);
        println!(
            "{:>4} | {:>6}/{:<5} | {:>3} | {:>5} | {:>6} | {:>8} | {:>7} | {}",
            world.expect_resource::<Tick>().0,
            grid.generated - grid.available,
            grid.generated,
            stock.ore,
            stock.metal,
            world.view::<&Miner>().iter().count(),
            world.view::<&Smelter>().iter().count(),
            report.changed.len(),
            report.construction.join(", ")
        );
    }
    assert_eq!(world.expect_resource::<Stock>().mined, 44);
    assert_eq!(world.expect_resource::<Stock>().smelted, 12);
    assert_eq!(world.expect_resource::<Stock>().metal, 8);
    println!("\nEvacuating settlement: ChildOf cascades through all seven assets.");
    world.despawn(settlement).unwrap();
    let stock = world.expect_resource::<Stock>();
    assert_conservation(&stock);
    assert_eq!(stock.reclaimed, 4);
    assert_eq!(stock.metal, 12);
    assert_eq!(world.view::<Entities>().iter().count(), 0);
    let mut reclaimed = world.expect_resource::<Cleanup>().0.clone();
    reclaimed.sort();
    assert_eq!(reclaimed.len(), 7);
    println!("Reclaimed: {}", reclaimed.join(", "));
    println!(
        "Conserved {} mined ore = {} stored ore + {} metal × 3; all {} metal recovered.",
        stock.mined, stock.ore, stock.smelted, stock.metal
    );
}
