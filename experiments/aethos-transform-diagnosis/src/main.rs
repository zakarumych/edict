use aethos_transform_diagnosis::{*, Backend};
use aethos_util::math::{Angle, Pos, Vec};
use std::{hint::black_box, time::Instant};

#[derive(Clone, Copy, Debug)]
enum Workload { Idle, Sparse, Roots, All }

fn local_value(alternate: bool) -> LocalTransform {
    LocalTransform { offset: Vec::new(if alternate { 3 } else { 5 }, 7), angle: Angle::new(if alternate { 0.025 } else { 0.05 }) }
}
fn root_value(alternate: bool) -> Transform {
    Transform { pos: Pos::default() + Vec::new(if alternate { 31 } else { 41 }, 50), angle: Angle::new(if alternate { 0.25 } else { 0.5 }) }
}
fn changed(fixture: &Fixture, workload: Workload) -> std::vec::Vec<usize> {
    let locals = fixture.nodes.iter().enumerate().filter_map(|(index, node)| node.parent.map(|_| index));
    match workload {
        Workload::Idle => vec![],
        Workload::Sparse => locals.step_by(100).collect(),
        Workload::Roots => fixture.nodes.iter().enumerate().filter_map(|(index, node)| node.parent.is_none().then_some(index)).collect(),
        Workload::All => locals.collect(),
    }
}
fn mutate<B: Backend>(backend: &mut B, changed: &[usize], workload: Workload, alternate: bool) {
    match workload {
        Workload::Roots => { let value = black_box(root_value(alternate)); for &index in changed { backend.set_root(index, value); } }
        Workload::Sparse | Workload::All => { let value = black_box(local_value(alternate)); for &index in changed { backend.set_local(index, value); } }
        Workload::Idle => {}
    }
}

fn verify<B: Backend>() {
    for workload in [Workload::Idle, Workload::Sparse, Workload::Roots, Workload::All] {
        let mut fixture = forest(1024);
        fixture.spawn_order.reverse();
        let mut backend = B::new(&fixture);
        backend.sync();
        assert_matches(&mut backend, &fixture);
        let indices = changed(&fixture, workload);
        for alternate in [false, true, false, true] {
            mutate(&mut backend, &indices, workload, alternate);
            for &index in &indices {
                match workload {
                    Workload::Roots => fixture.nodes[index].global = root_value(alternate),
                    _ => fixture.nodes[index].local = local_value(alternate),
                }
            }
            backend.sync();
            assert_matches(&mut backend, &fixture);
            backend.sync();
            assert_matches(&mut backend, &fixture);
        }
        for (index, parent) in [(2, 85), (1, 170), (2, 0), (1, 0)] {
            backend.reparent(index, parent);
            fixture.nodes[index].parent = Some(parent);
            backend.sync();
            assert_matches(&mut backend, &fixture);
            backend.sync();
            assert_matches(&mut backend, &fixture);
        }
    }
}

fn stats(values: &mut [f64]) -> (f64, f64, f64) {
    values.sort_by(f64::total_cmp);
    (values[0], values[values.len()/2], values.iter().sum::<f64>() / values.len() as f64)
}

fn run<B: Backend>(name: &str, phase: fn() -> (u64,u64), count: usize, samples: usize, frames: usize, verify_only: bool) {
    verify::<B>();
    if verify_only { println!("verified {name}"); return; }
    for workload in [Workload::Idle, Workload::Sparse, Workload::Roots, Workload::All] {
        let fixture = forest(count);
        let indices = changed(&fixture, workload);
        let mut backend = B::new(&fixture);
        backend.sync();
        assert_matches(&mut backend, &fixture);
        let mut alternate = false;
        for _ in 0..64 { alternate = !alternate; mutate(&mut backend, &indices, workload, alternate); backend.sync(); }
        let mut total = vec![];
        let mut mutation = vec![];
        let mut synchronization = vec![];
        let mut marking = vec![];
        let mut updating = vec![];
        for _ in 0..samples {
            let start = Instant::now();
            for _ in 0..frames {
                alternate = !alternate;
                mutate(&mut backend, &indices, workload, alternate);
                backend.sync();
                black_box(backend.global(count - 1));
            }
            total.push(start.elapsed().as_nanos() as f64 / frames as f64);
            let (mut m, mut s, mut mark, mut update) = (0u128,0u128,0u128,0u128);
            for _ in 0..frames {
                alternate = !alternate;
                let start = Instant::now();
                mutate(&mut backend, &indices, workload, alternate);
                let mutated = start.elapsed();
                backend.sync();
                let synced = start.elapsed();
                m += mutated.as_nanos();
                s += (synced - mutated).as_nanos();
                let (a,b) = phase();
                mark += a as u128; update += b as u128;
                black_box(backend.global(count - 1));
            }
            mutation.push(m as f64 / frames as f64);
            synchronization.push(s as f64 / frames as f64);
            marking.push(mark as f64 / frames as f64);
            updating.push(update as f64 / frames as f64);
        }
        let (min, med, mean) = stats(&mut total);
        let (_, mut_med, _) = stats(&mut mutation);
        let (_, sync_med, _) = stats(&mut synchronization);
        let (_, mark_med, _) = stats(&mut marking);
        let (_, update_med, _) = stats(&mut updating);
        println!("{name},{workload:?},{count},{},{min:.1},{med:.1},{mean:.1},{mut_med:.1},{sync_med:.1},{mark_med:.1},{update_med:.1}", indices.len());
    }
}

fn main() {
    let args: std::vec::Vec<_> = std::env::args().collect();
    let value = |key: &str| args.windows(2).find(|a| a[0] == key).map(|a| a[1].as_str());
    let count = value("--count").unwrap_or("16384").parse().unwrap();
    let samples = value("--samples").unwrap_or("7").parse().unwrap();
    let frames = value("--frames").unwrap_or("64").parse().unwrap();
    let filter = value("--backend").unwrap_or("all");
    let verify_only = args.iter().any(|s| s == "--verify-only");
    if args.iter().any(|s| s == "--idle-writes") {
        println!("edict_3 Global writes during three idle syncs after initial sync: {:?}", edict_3_backend::idle_writes(count));
        println!("edict_3_alt Global writes during three idle syncs after initial sync: {:?}", edict_3_alt_backend::idle_writes(count));
        return;
    }
    println!("backend,workload,count,changed,total_min_ns,total_median_ns,total_mean_ns,mutation_median_ns,sync_median_ns,mark_median_ns,update_median_ns");
    macro_rules! bench { ($name:literal, $module:ident, $phase:expr) => {
        if filter == "all" || filter == $name { run::<$module::Adapter>($name, $phase, count, samples, frames, verify_only); }
    }; }
    bench!("bevy", bevy_backend, bevy_backend::phases);
    bench!("edict_bevy", edict_bevy_backend, edict_bevy_backend::phases);
    bench!("edict_alt", edict_alt_backend, edict_alt_backend::phases);
    bench!("edict_narrow", edict_narrow_backend, edict_narrow_backend::phases);
    bench!("edict_located", edict_located_backend, edict_located_backend::phases);
    bench!("edict", edict_backend, || (0,0));
    bench!("edict_3", edict_3_backend, || (0,0));
    bench!("edict_3_alt", edict_3_alt_backend, || (0,0));
}
