use edict::{
    entity::AliveEntity,
    query::{AsDefaultQuery, Fetch, Query, Read},
};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

#[derive(edict::component::Component, bevy_ecs::component::Component)]
struct Value(u64);
#[derive(edict::component::Component, bevy_ecs::component::Component)]
struct Parent(u64);
#[derive(edict::component::Component, bevy_ecs::component::Component)]
struct Children(u64);
#[derive(edict::component::Component, bevy_ecs::component::Component)]
struct Depth<const N: usize>;

const N: usize = 16_384;

// Safety: idx belongs to the archetype backing fetch, and its read borrow is
// held throughout this call. Used only with unfiltered Read/OptionQuery tuples.
unsafe fn read_fetch_item<'a, F: Fetch<'a>>(fetch: &mut F, idx: u32) -> F::Item {
    // Matches edict's private CHUNK_LEN = 0x100 in src/archetype.rs.
    let chunk = idx / 256;
    unsafe {
        assert!(fetch.visit_chunk(chunk));
        fetch.touch_chunk(chunk);
        assert!(fetch.visit_item(idx));
        fetch.get_item(idx)
    }
}

fn measure(name: &str, ms: u64, filter: &str, expected: u64, mut run: impl FnMut() -> u64) {
    if !name.contains(filter) {
        return;
    }
    assert_eq!(run(), expected, "{name}: checksum");
    if ms == 0 {
        println!("{name:42} checksum OK");
        return;
    }
    let mut samples = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let mut batches = 0;
        while start.elapsed() < Duration::from_millis(ms) {
            black_box(run());
            batches += 1;
        }
        samples.push(start.elapsed().as_nanos() as f64 / (batches * N) as f64);
    }
    assert_eq!(run(), expected, "{name}: final checksum");
    samples.sort_by(f64::total_cmp);
    println!(
        "{name:42} {:9.3} ns/entity  [{:.3}, {:.3}]",
        samples[2], samples[0], samples[4]
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let ms = args.next().map(|s| s.parse().unwrap()).unwrap_or(100);
    let filter = args.next().unwrap_or_default();
    println!("{N} entities; four depth archetypes; five {ms} ms samples; median [min,max]");
    for shuffled in [false, true] {
        let mut ew = edict::world::World::new();
        let mut bw = bevy_ecs::world::World::new();
        let mut ei = Vec::new();
        let mut bi = Vec::new();
        let mut tuple_sum = 0;
        for i in 0..N {
            let depth = i % 4;
            let v = i as u64;
            tuple_sum += v + if depth > 0 { v + 3 } else { 0 } + if depth < 3 { v + 5 } else { 0 };
            macro_rules! spawn {
                ($d:literal, $($extra:expr,)*) => {{
                    ei.push(ew.spawn((Value(v), Depth::<$d>, $($extra,)* )).id());
                    bi.push(bw.spawn((Value(v), Depth::<$d>, $($extra,)* )).id());
                }};
            }
            match depth {
                0 => spawn!(0, Children(v + 5),),
                1 => spawn!(1, Parent(v + 3), Children(v + 5),),
                2 => spawn!(2, Parent(v + 3), Children(v + 5),),
                _ => spawn!(3, Parent(v + 3),),
            }
        }
        if shuffled {
            let mut rng = 0x4d59_5df4_d0f3_3173u64;
            for i in (1..N).rev() {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                let j = rng as usize % (i + 1);
                ei.swap(i, j);
                bi.swap(i, j);
            }
        }
        let order = if shuffled { "shuffled" } else { "interleaved" };
        let sum = (N as u64) * ((N - 1) as u64) / 2;
        macro_rules! bench {
            ($name:expr, $expected:expr, $body:expr) => {
                measure(&format!("{order}/{}", $name), ms, &filter, $expected, $body)
            };
        }
        bench!("edict/lookup", N as u64, || {
            for &id in black_box(&ei) {
                black_box(ew.lookup(id).unwrap());
            }
            N as u64
        });
        bench!("bevy/lookup", N as u64, || {
            for &id in black_box(&bi) {
                black_box(bw.entities().get(id).unwrap());
            }
            N as u64
        });
        {
            let mut view = ew.view::<&Value>();
            let view = view.lock();
            bench!("edict/read_id", sum, || black_box(&ei)
                .iter()
                .map(|&id| view.try_get(id).unwrap().0)
                .sum());
            let locs: Vec<_> = ei.iter().map(|&id| ew.lookup(id).unwrap()).collect();
            bench!("edict/read_loc", sum, || black_box(&locs)
                .iter()
                .map(|&loc| view.try_get(loc).unwrap().0)
                .sum());
            bench!("edict/read_iter", sum, || black_box(&view)
                .iter()
                .map(|v| v.0)
                .sum());

            // EntityLoc::locate ignores its EntitySet argument and returns its
            // stored location. World::entities is private; this empty set only
            // supplies the unused public trait argument for located handles.
            let unused_entities = edict::entity::EntitySet::new();
            let query = Read::<Value>;
            let archetypes = ew.archetypes();
            let epoch = ew.epoch();
            let mut fetches: Vec<_> = archetypes
                .iter()
                .enumerate()
                .map(|(index, arch)| {
                    if !query.visit_archetype(arch) {
                        return None;
                    }
                    // Safety: the locked view holds the same read access. No world
                    // mutation occurs while these archetype borrows/fetches exist.
                    unsafe {
                        query.access_archetype(arch, |_, _| {});
                        assert!(query.visit_archetype_late(arch));
                        Some(query.fetch(index as u32, arch, epoch))
                    }
                })
                .collect();
            for &entity in &locs {
                let loc = entity.locate(&unused_entities);
                assert!(fetches[loc.arch as usize].is_some());
                assert!(view.try_get(entity).is_ok());
            }
            bench!("edict/read_loc_fetch_each", sum, || black_box(&locs)
                .iter()
                .map(|&entity| {
                    let loc = entity.locate(&unused_entities);
                    // Safety: location/matching archetype validated outside timing;
                    // immutable world borrow keeps indices and storage stable. The
                    // original locked view guards this unfiltered read component.
                    unsafe {
                        let mut fetch =
                            query.fetch(loc.arch, &archetypes[loc.arch as usize], epoch);
                        read_fetch_item(&mut fetch, loc.idx).0
                    }
                })
                .sum());
            bench!("edict/read_loc_cached_fetch", sum, || black_box(&locs)
                .iter()
                .map(|&entity| {
                    let loc = entity.locate(&unused_entities);
                    let fetch = fetches[loc.arch as usize].as_mut().unwrap();
                    // Safety: fetch and index belong to the same validated
                    // archetype; locked read view and immutable world borrow live.
                    unsafe { read_fetch_item(fetch, loc.idx).0 }
                })
                .sum());
        }
        {
            let mut state = bw.query::<&Value>();
            let q = state.query(&bw);
            bench!("bevy/read_id", sum, || black_box(&bi)
                .iter()
                .map(|&id| q.get(id).unwrap().0)
                .sum());
            bench!("bevy/read_iter", sum, || black_box(&q)
                .iter()
                .map(|v| v.0)
                .sum());
        }
        {
            let mut view = ew.view::<(Option<&Parent>, Option<&Children>, Option<&Value>)>();
            let view = view.lock();
            bench!("edict/tuple_id", tuple_sum, || black_box(&ei)
                .iter()
                .map(|&id| {
                    let (p, c, v) = view.try_get(id).unwrap();
                    p.map_or(0, |p| p.0) + c.map_or(0, |c| c.0) + v.map_or(0, |v| v.0)
                })
                .sum());
            let locs: Vec<_> = ei.iter().map(|&id| ew.lookup(id).unwrap()).collect();
            bench!("edict/tuple_loc", tuple_sum, || black_box(&locs)
                .iter()
                .map(|&loc| {
                    let (p, c, v) = view.try_get(loc).unwrap();
                    p.map_or(0, |p| p.0) + c.map_or(0, |c| c.0) + v.map_or(0, |v| v.0)
                })
                .sum());

            let unused_entities = edict::entity::EntitySet::new();
            let query = <(Option<&Parent>, Option<&Children>, Option<&Value>)>::default_query();
            let archetypes = ew.archetypes();
            let epoch = ew.epoch();
            let mut fetches: Vec<_> = archetypes
                .iter()
                .enumerate()
                .map(|(index, arch)| {
                    assert!(query.visit_archetype(arch));
                    // Safety: the locked tuple view guards all optional read
                    // components; immutable world borrow keeps their storage live.
                    unsafe {
                        query.access_archetype(arch, |_, _| {});
                        assert!(query.visit_archetype_late(arch));
                        query.fetch(index as u32, arch, epoch)
                    }
                })
                .collect();
            for &entity in &locs {
                assert!(view.try_get(entity).is_ok());
            }
            bench!("edict/tuple_loc_cached_fetch", tuple_sum, || black_box(
                &locs
            )
            .iter()
            .map(|&entity| {
                let loc = entity.locate(&unused_entities);
                // Safety: current location and cached fetch share an archetype;
                // tuple has no filters, and its locked read view remains live.
                let (p, c, v) =
                    unsafe { read_fetch_item(&mut fetches[loc.arch as usize], loc.idx) };
                p.map_or(0, |p| p.0) + c.map_or(0, |c| c.0) + v.map_or(0, |v| v.0)
            })
            .sum());
        }
        {
            let mut state = bw.query::<(Option<&Parent>, Option<&Children>, Option<&Value>)>();
            let q = state.query(&bw);
            bench!("bevy/tuple_id", tuple_sum, || black_box(&bi)
                .iter()
                .map(|&id| {
                    let (p, c, v) = q.get(id).unwrap();
                    p.map_or(0, |p| p.0) + c.map_or(0, |c| c.0) + v.map_or(0, |v| v.0)
                })
                .sum());
        }
        {
            let mut view = ew.view::<&mut Value>();
            let mut view = view.lock();
            bench!("edict/write_id", 7 * N as u64, || {
                let value = black_box(7);
                let mut sum = 0;
                for &id in black_box(&ei) {
                    let v = view.try_get_mut(id).unwrap();
                    v.0 = value;
                    sum += v.0;
                }
                sum
            });
            let locs: Vec<_> = ei.iter().map(|&id| ew.lookup(id).unwrap()).collect();
            bench!("edict/write_loc", 7 * N as u64, || {
                let value = black_box(7);
                let mut sum = 0;
                for &loc in black_box(&locs) {
                    let v = view.try_get_mut(loc).unwrap();
                    v.0 = value;
                    sum += v.0;
                }
                sum
            });
            bench!("edict/write_iter", 7 * N as u64, || {
                let value = black_box(7);
                let mut sum = 0;
                for v in view.iter_mut() {
                    v.0 = value;
                    sum += v.0;
                }
                sum
            });
        }
        {
            let mut state = bw.query::<&mut Value>();
            let mut q = state.query_mut(&mut bw);
            bench!("bevy/write_id", 7 * N as u64, || {
                let value = black_box(7);
                let mut sum = 0;
                for &id in black_box(&bi) {
                    let mut v = q.get_mut(id).unwrap();
                    v.0 = value;
                    sum += v.0;
                }
                sum
            });
            bench!("bevy/write_iter", 7 * N as u64, || {
                let value = black_box(7);
                let mut sum = 0;
                for mut v in q.iter_mut() {
                    v.0 = value;
                    sum += v.0;
                }
                sum
            });
        }
        bench!("edict/world_write", 9 * N as u64, || {
            let value = black_box(9);
            let mut sum = 0;
            for &id in black_box(&ei) {
                let v = ew.get::<&mut Value>(id).unwrap();
                v.0 = value;
                sum += v.0;
            }
            sum
        });
        bench!("bevy/world_write", 9 * N as u64, || {
            let value = black_box(9);
            let mut sum = 0;
            for &id in black_box(&bi) {
                let mut v = bw.get_mut::<Value>(id).unwrap();
                v.0 = value;
                sum += v.0;
            }
            sum
        });
    }
}
