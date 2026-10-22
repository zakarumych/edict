use std::panic::{AssertUnwindSafe, catch_unwind};

use edict::{
    component::Component,
    epoch::EpochId,
    query::{Modified, Read},
    world::World,
};

#[derive(Component)]
struct Value(i32);
#[derive(Component)]
struct Weight(i32);
#[derive(Component)]
struct Selected;

fn fixture() -> World {
    let mut world = World::new();
    world
        .spawn_batch((0..620).map(|i| (Value(i), Weight(3), Selected)))
        .spawn_all();
    world
        .spawn_batch((620..935).map(|i| (Value(i), Selected)))
        .spawn_all();
    world
        .spawn_batch((935..977).map(|i| (Value(i),)))
        .spawn_all();
    world
}

fn modified(world: &World, epoch: EpochId) -> Vec<i32> {
    let mut values: Vec<_> = world
        .view_with(Modified::<Read<Value>>::new(epoch))
        .into_iter()
        .map(|v| v.0)
        .collect();
    values.sort_unstable();
    values
}

#[test]
fn filtered_read_next_visits_chunks_for_small_and_spanning_batches() {
    let world = fixture();
    for size in [1, 17, 128, 255, 256, 257, 512, 700, u32::MAX] {
        let view = world.view::<&Value>().with::<Selected>();
        let mut iter = view.iter_batched(size);
        let mut actual = Vec::new();
        while let Some(batch) = iter.next() {
            assert!(!batch.is_empty());
            assert!(batch.len() <= size as usize);
            actual.extend(batch.iter().map(|v| v.0));
        }
        actual.sort_unstable();
        assert_eq!(actual, (0..935).collect::<Vec<_>>());
    }
}

#[test]
fn filtered_read_fold_visits_chunks_across_archetypes() {
    let world = fixture();
    for size in [17, 128, 256, 257, 512, 700] {
        let mut values = world
            .view::<&Value>()
            .with::<Selected>()
            .into_iter_batched(size)
            .fold(Vec::new(), |mut values, batch| {
                values.extend(batch.iter().map(|v| v.0));
                values
            });
        values.sort_unstable();
        assert_eq!(values, (0..935).collect::<Vec<_>>());
    }
}

#[test]
fn partial_next_then_fold_preserves_chunk_state_and_remaining_ranges() {
    let world = fixture();
    for size in [17, 128, 256, 257, 512, 700] {
        let view = world.view::<&Value>().with::<Selected>();
        let mut iter = view.iter_batched(size);
        let first: Vec<_> = iter.next().unwrap().iter().map(|v| v.0).collect();
        let mut values = iter.fold(first, |mut values, batch| {
            values.extend(batch.iter().map(|v| v.0));
            values
        });
        values.sort_unstable();
        assert_eq!(values, (0..935).collect::<Vec<_>>());
    }
}

#[test]
fn filtered_mutable_batches_touch_matching_entities_only() {
    for size in [17, 128, 256, 257, 512, 700] {
        let world = fixture();
        let epoch = world.epoch();
        {
            let mut view = world.view::<(&mut Value, &Weight)>().with::<Selected>();
            let mut iter = view.iter_batched_mut(size);
            while let Some((values, weights)) = iter.next() {
                assert_eq!(values.len(), weights.len());
                for (value, weight) in values.iter_mut().zip(weights) {
                    value.0 += weight.0;
                }
            }
        }
        assert_eq!(modified(&world, epoch), (3..623).collect::<Vec<_>>());
        let mut untouched: Vec<_> = world
            .view::<&Value>()
            .without::<Weight>()
            .into_iter()
            .map(|v| v.0)
            .collect();
        untouched.sort_unstable();
        assert_eq!(untouched, (620..977).collect::<Vec<_>>());
    }
}

#[test]
fn filtered_mutable_fold_after_next_visits_each_entity_once() {
    let world = fixture();
    let epoch = world.epoch();
    let mut iter = world
        .view::<&mut Value>()
        .with::<Selected>()
        .into_iter_batched(257);
    for value in iter.next().unwrap() {
        value.0 += 1000;
    }
    let visited = iter.fold(257, |count, batch| {
        for value in batch.iter_mut() {
            value.0 += 1000;
        }
        count + batch.len()
    });
    assert_eq!(visited, 935);
    assert_eq!(modified(&world, epoch), (1000..1935).collect::<Vec<_>>());
}

#[test]
fn exact_size_and_size_hint_shrink_per_returned_batch() {
    let world = fixture();
    for size in [1, 17, 128, 256, 257, 512, 700, u32::MAX] {
        let view = world.view::<&Value>().with::<Selected>();
        let mut iter = view.iter_batched(size);
        let mut remaining = 620u32.div_ceil(size) as usize + 315u32.div_ceil(size) as usize;
        loop {
            assert_eq!(iter.len(), remaining);
            assert_eq!(iter.size_hint(), (remaining, Some(remaining)));
            match iter.next() {
                Some(_) => remaining -= 1,
                None => {
                    assert_eq!(remaining, 0);
                    break;
                }
            }
        }
        assert!(iter.next().is_none());
        assert_eq!(iter.len(), 0);
    }
}

#[test]
fn empty_filtered_archetypes_have_zero_batches() {
    let mut world = World::new();
    let empty = world.spawn((Value(1), Selected)).id();
    world.despawn(empty).unwrap();
    world.spawn_one(Value(2));
    let view = world.view::<&Value>().with::<Selected>();
    let mut iter = view.iter_batched(17);
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.size_hint(), (0, Some(0)));
    assert!(iter.next().is_none());
    assert_eq!(iter.fold(0, |count, _| count + 1), 0);
}

#[test]
fn zero_batch_size_is_rejected_before_runtime_borrowing() {
    let world = fixture();
    let view = world.view::<&Value>().with::<Selected>();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = view.iter_batched(0);
        }))
        .is_err()
    );
    assert_eq!(world.view::<&mut Value>().into_iter().count(), 977);
    let mut view = world.view::<&mut Value>().with::<Selected>();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = view.iter_batched_mut(0);
        }))
        .is_err()
    );
    assert_eq!(world.view::<&Value>().into_iter().count(), 977);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = world
                .view::<&mut Value>()
                .with::<Selected>()
                .into_iter_batched(0);
        }))
        .is_err()
    );
    assert_eq!(world.view::<&mut Value>().into_iter().count(), 977);
    let mut acquired = world.view::<&mut Value>().with::<Selected>();
    assert_eq!(acquired.iter_mut().count(), 935);
    assert!(
        catch_unwind(AssertUnwindSafe(move || {
            let _ = acquired.into_iter_batched(0);
        }))
        .is_err()
    );
    assert_eq!(world.view::<&mut Value>().into_iter().count(), 977);
}
