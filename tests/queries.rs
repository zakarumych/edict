use std::{
    any::TypeId,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
};

use edict::{
    EntityError,
    component::Component,
    entity::EntityId,
    epoch::EpochId,
    query::{Alt, And, Cpy, Entities, Modified, Not, Or, Read, With, Without, Xor},
    world::World,
};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct A(i32);
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct B(i32);
#[derive(Component)]
struct Tag;

fn changed(world: &World, epoch: EpochId) -> Vec<i32> {
    let mut values: Vec<_> = world
        .view_with(Modified::<Read<A>>::new(epoch))
        .into_iter()
        .map(|a| a.0)
        .collect();
    values.sort_unstable();
    values
}

fn combinations() -> (World, [EntityId; 4]) {
    let mut world = World::new();
    let neither = world.spawn_empty().id();
    let a = world.spawn_one(A(1)).id();
    let b = world.spawn_one(B(2)).id();
    let both = world.spawn((A(3), B(4))).id();
    (world, [neither, a, b, both])
}

#[test]
fn empty_world_queries_are_empty() {
    let world = World::new();
    assert_eq!(world.view::<Entities>().into_iter().count(), 0);
    assert_eq!(world.view::<Option<&A>>().into_iter().count(), 0);
    assert_eq!(world.view::<&A>().into_iter().count(), 0);
}

#[test]
fn unit_and_entities_include_empty_entities() {
    let (world, ids) = combinations();
    assert_eq!(world.new_view().into_iter().count(), 4);
    let found: Vec<_> = world
        .view::<Entities>()
        .into_iter()
        .map(|e| e.id())
        .collect();
    assert_eq!(found.len(), 4);
    assert!(ids.into_iter().all(|id| found.contains(&id)));
}

#[test]
fn tuple_query_requires_all_components() {
    let (world, _) = combinations();
    let values: Vec<_> = world
        .view::<(&A, &B)>()
        .into_iter()
        .map(|(a, b)| (a.0, b.0))
        .collect();
    assert_eq!(values, [(3, 4)]);
}

#[test]
fn optional_tuple_preserves_missing_components() {
    let (world, _) = combinations();
    let mut values: Vec<_> = world
        .view::<(Option<&A>, Option<&B>)>()
        .into_iter()
        .map(|(a, b)| (a.map(|a| a.0), b.map(|b| b.0)))
        .collect();
    values.sort_unstable();
    assert_eq!(
        values,
        [
            (None, None),
            (None, Some(2)),
            (Some(1), None),
            (Some(3), Some(4))
        ]
    );
}

#[test]
fn optional_mutable_query_only_changes_present_values() {
    let (mut world, ids) = combinations();
    for (a, b) in world.view::<(Option<&mut A>, Option<&B>)>() {
        if let Some(a) = a {
            a.0 += b.map_or(10, |b| b.0);
        }
    }
    assert_eq!(world.get::<Cpy<A>>(ids[1]).unwrap(), A(11));
    assert_eq!(world.get::<Cpy<A>>(ids[3]).unwrap(), A(7));
    assert_eq!(world.get::<&A>(ids[2]), Err(EntityError::Mismatch));
}

#[test]
fn with_without_filters_follow_presence_truth_table() {
    let (world, _) = combinations();
    assert_eq!(world.new_view().with::<A>().into_iter().count(), 2);
    assert_eq!(world.new_view().without::<A>().into_iter().count(), 2);
    assert_eq!(
        world
            .new_view()
            .with::<A>()
            .without::<B>()
            .into_iter()
            .count(),
        1
    );
    assert_eq!(
        world
            .new_view()
            .with::<A>()
            .without::<A>()
            .into_iter()
            .count(),
        0
    );
}

#[test]
fn boolean_and_requires_every_arm() {
    let (world, _) = combinations();
    let values: Vec<_> = world
        .view::<And<(&A, &B)>>()
        .into_iter()
        .map(|(a, b)| (a.unwrap().0, b.unwrap().0))
        .collect();
    assert_eq!(values, [(3, 4)]);
}

#[test]
fn boolean_or_yields_each_matching_entity_once() {
    let (world, _) = combinations();
    let mut values: Vec<_> = world
        .view::<Or<(&A, &B)>>()
        .into_iter()
        .map(|(a, b)| (a.map(|a| a.0), b.map(|b| b.0)))
        .collect();
    values.sort_unstable();
    assert_eq!(
        values,
        [(None, Some(2)), (Some(1), None), (Some(3), Some(4))]
    );
}

#[test]
fn boolean_xor_excludes_entities_matching_both_arms() {
    let (world, _) = combinations();
    let mut values: Vec<_> = world
        .view::<Xor<(&A, &B)>>()
        .into_iter()
        .map(|(a, b)| (a.map(|a| a.0), b.map(|b| b.0)))
        .collect();
    values.sort_unstable();
    assert_eq!(values, [(None, Some(2)), (Some(1), None)]);
}

#[test]
fn nested_negation_and_boolean_filters() {
    let (world, _) = combinations();
    assert_eq!(
        world
            .view::<Not<Or<(With<A>, With<B>)>>>()
            .into_iter()
            .count(),
        1
    );
    assert_eq!(world.view::<Not<Not<With<A>>>>().into_iter().count(), 2);
    assert_eq!(
        world
            .view::<And<(Without<A>, Without<B>)>>()
            .into_iter()
            .count(),
        1
    );
}

#[test]
fn three_arm_xor_means_exactly_one_match() {
    let mut world = World::new();
    for mask in 0..8 {
        let mut entity = world.spawn_empty();
        if mask & 1 != 0 {
            entity = entity.insert(A(mask)).unwrap();
        }
        if mask & 2 != 0 {
            entity = entity.insert(B(mask)).unwrap();
        }
        if mask & 4 != 0 {
            let _ = entity.insert(Tag);
        }
    }
    assert_eq!(
        world
            .view::<Xor<(With<A>, With<B>, With<Tag>)>>()
            .into_iter()
            .count(),
        3
    );
    assert_eq!(
        world
            .view::<Or<(With<A>, With<B>, With<Tag>)>>()
            .into_iter()
            .count(),
        7
    );
    assert_eq!(
        world
            .view::<And<(With<A>, With<B>, With<Tag>)>>()
            .into_iter()
            .count(),
        1
    );
}

#[test]
fn get_distinguishes_mismatch_from_dead_entity() {
    let (mut world, ids) = combinations();
    assert_eq!(world.get::<&A>(ids[0]), Err(EntityError::Mismatch));
    world.despawn(ids[1]).unwrap();
    assert_eq!(world.get::<&A>(ids[1]), Err(EntityError::NoSuchEntity));
    assert_eq!(
        world.view::<&A>().try_get(ids[0]),
        Err(EntityError::Mismatch)
    );
    assert_eq!(
        world.view::<&A>().try_get(ids[1]),
        Err(EntityError::NoSuchEntity)
    );
}

#[test]
fn copied_query_returns_owned_values_without_marking_modified() {
    let (mut world, ids) = combinations();
    let epoch = world.epoch();
    let copy = world.get::<Cpy<A>>(ids[1]).unwrap();
    assert_eq!(copy, A(1));
    assert!(changed(&world, epoch).is_empty());
    world.insert(ids[1], A(9)).unwrap();
    assert_eq!(copy, A(1));
    assert_eq!(changed(&world, epoch), [9]);
}

#[test]
fn modified_threshold_is_exclusive_and_reads_do_not_touch() {
    let mut world = World::new();
    let before = world.epoch();
    let id = world.spawn_one(A(5)).id();
    assert_eq!(changed(&world, before), [5]);
    let after = world.epoch();
    assert_eq!(world.get::<&A>(id).unwrap().0, 5);
    assert_eq!(world.view::<&A>().into_iter().count(), 1);
    assert!(changed(&world, after).is_empty());
}

#[test]
fn write_fetch_marks_component_without_value_change() {
    let mut world = World::new();
    let id = world.spawn_one(A(5)).id();
    let epoch = world.epoch();
    let _ = world.get::<&mut A>(id).unwrap();
    assert_eq!(changed(&world, epoch), [5]);
}

#[test]
fn alt_read_and_drop_do_not_mark_modified() {
    let mut world = World::new();
    let id = world.spawn_one(A(5)).id();
    let epoch = world.epoch();
    {
        let value = world.get::<Alt<A>>(id).unwrap();
        assert_eq!(value.0, 5);
    }
    for value in world.view::<Alt<A>>() {
        assert_eq!(value.0, 5);
    }
    assert!(changed(&world, epoch).is_empty());
}

#[test]
fn alt_mutation_marks_only_dereferenced_entities_across_chunks() {
    let mut world = World::new();
    world.spawn_batch((0..257).map(|i| (A(i),))).spawn_all();
    let epoch = world.epoch();
    for mut a in world.view::<Alt<A>>() {
        if a.0 % 64 == 0 {
            a.0 += 1000;
        }
    }
    assert_eq!(changed(&world, epoch), [1000, 1064, 1128, 1192, 1256]);
}

#[test]
fn insert_and_remove_unrelated_component_preserve_change_epoch() {
    let mut world = World::new();
    let id = world.spawn_one(A(8)).id();
    let epoch = world.epoch();
    world.insert(id, B(2)).unwrap();
    assert!(changed(&world, epoch).is_empty());
    world.remove::<B>(id).unwrap();
    assert!(changed(&world, epoch).is_empty());
    assert_eq!(world.get::<Cpy<A>>(id).unwrap(), A(8));
}

#[test]
fn modified_entity_survives_archetype_transition() {
    let mut world = World::new();
    let id = world.spawn_one(A(8)).id();
    let epoch = world.epoch();
    world.get::<&mut A>(id).unwrap().0 = 9;
    world.insert(id, B(2)).unwrap();
    assert_eq!(changed(&world, epoch), [9]);
    world.remove::<B>(id).unwrap();
    assert_eq!(changed(&world, epoch), [9]);
}

#[test]
fn swap_removal_preserves_survivor_epoch_and_location() {
    let mut world = World::new();
    let ids: Vec<_> = world
        .spawn_batch((0..129).map(|i| (A(i),)))
        .map(|e| e.id())
        .collect();
    let epoch = world.epoch();
    world.get::<&mut A>(ids[128]).unwrap().0 = 500;
    world.despawn(ids[0]).unwrap();
    assert_eq!(changed(&world, epoch), [500]);
    assert_eq!(world.get::<Cpy<A>>(ids[128]).unwrap(), A(500));
    world.despawn(ids[1]).unwrap();
    assert_eq!(changed(&world, epoch), [500]);
}

#[test]
fn modified_mutable_query_filters_before_touching() {
    let mut world = World::new();
    let ids: Vec<_> = world
        .spawn_batch((0..130).map(|i| (A(i),)))
        .map(|e| e.id())
        .collect();
    let epoch = world.epoch();
    world.insert(ids[65], A(500)).unwrap();
    let newer = world.epoch();
    for a in world.view_with(Modified::<edict::query::Write<A>>::new(epoch)) {
        a.0 += 1;
    }
    assert_eq!(changed(&world, newer), [501]);
}

#[test]
fn readonly_views_can_coexist_and_reiterate() {
    let (world, _) = combinations();
    let first = world.view::<&A>();
    let second = world.view::<&A>();
    assert_eq!(first.iter().map(|a| a.0).sum::<i32>(), 4);
    assert_eq!(second.iter().map(|a| a.0).sum::<i32>(), 4);
    assert_eq!(first.iter().count(), 2);
}

#[test]
fn runtime_borrow_conflict_recovers_after_view_drop() {
    let (world, _) = combinations();
    let mut writer = world.view::<&mut A>();
    assert_eq!(writer.iter_mut().count(), 2);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            world.view::<&A>().into_iter().count();
        }))
        .is_err()
    );
    drop(writer);
    assert_eq!(world.view::<&A>().into_iter().count(), 2);
    assert_eq!(world.view::<&mut A>().into_iter().count(), 2);
}

#[test]
fn duplicate_write_and_read_write_aliases_panic_without_leaking_borrows() {
    let (world, _) = combinations();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            world.view::<(&mut A, &mut A)>().into_iter().count();
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            world.view::<(&A, &mut A)>().into_iter().count();
        }))
        .is_err()
    );
    assert_eq!(world.view::<&mut A>().into_iter().count(), 2);
}

#[test]
fn disjoint_archetype_filters_allow_simultaneous_writers() {
    let (world, _) = combinations();
    let mut with = world.view::<&mut A>().with::<B>();
    let mut without = world.view::<&mut A>().without::<B>();
    let left = with.iter_mut().next().unwrap();
    let right = without.iter_mut().next().unwrap();
    left.0 += 10;
    right.0 += 20;
    assert_eq!((left.0, right.0), (13, 21));
}

#[test]
fn explicit_unlock_releases_runtime_borrow() {
    let (world, _) = combinations();
    let mut reader = world.view::<&A>();
    assert_eq!(reader.iter().count(), 2);
    reader.unlock();
    assert_eq!(world.view::<&mut A>().into_iter().count(), 2);
    assert_eq!(reader.iter().count(), 2);
}

#[test]
fn batched_read_visits_every_entity_across_archetypes() {
    let mut world = World::new();
    world.spawn_batch((0..257).map(|i| (A(i),))).spawn_all();
    world
        .spawn_batch((257..391).map(|i| (A(i), B(i))))
        .spawn_all();
    for size in [1, 7, 64, 128, 1000] {
        let view = world.view::<&A>();
        let mut values = Vec::new();
        for batch in view.iter_batched(size) {
            assert!(!batch.is_empty());
            assert!(batch.len() <= size as usize);
            values.extend(batch.iter().map(|a| a.0));
        }
        values.sort_unstable();
        assert_eq!(values, (0..391).collect::<Vec<_>>());
    }
}

#[test]
fn batched_mutation_updates_values_and_change_detection() {
    let mut world = World::new();
    world
        .spawn_batch((0..257).map(|i| (A(i), B(2))))
        .spawn_all();
    let epoch = world.epoch();
    {
        let mut view = world.view::<(&mut A, &B)>();
        for (a, b) in view.iter_batched_mut(17) {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter_mut().zip(b) {
                a.0 += b.0;
            }
        }
    }
    let mut values: Vec<_> = world.view::<&A>().into_iter().map(|a| a.0).collect();
    values.sort_unstable();
    assert_eq!(values, (2..259).collect::<Vec<_>>());
    assert_eq!(changed(&world, epoch), (2..259).collect::<Vec<_>>());
}

#[derive(Component)]
#[edict(borrow(dyn fmt::Display))]
struct Label(&'static str);
impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[test]
fn borrow_any_all_and_specific_component_respect_available_borrows() {
    let mut world = World::new();
    world.spawn_one(Label("first"));
    world.spawn((Label("second"), A(3)));
    world.spawn_one(A(4));
    let mut any: Vec<_> = world
        .new_view()
        .borrow_any::<dyn fmt::Display + Sync>()
        .into_iter()
        .map(|v| v.to_string())
        .collect();
    any.sort();
    assert_eq!(any, ["first", "second"]);
    let mut all: Vec<_> = world
        .new_view()
        .borrow_all::<dyn fmt::Display + Sync>()
        .into_iter()
        .flat_map(|values| values.map(|v| v.to_string()))
        .collect();
    all.sort();
    assert_eq!(all, any);
    let mut specific: Vec<_> = world
        .new_view()
        .borrow_one::<dyn fmt::Display + Sync>(TypeId::of::<Label>())
        .into_iter()
        .map(|v| v.to_string())
        .collect();
    specific.sort();
    assert_eq!(specific, any);
}

#[test]
fn boolean_modified_arms_reset_item_and_chunk_matches() {
    let mut world = World::new();
    let ids: Vec<_> = world
        .spawn_batch((0..770).map(|i| (A(i), B(i))))
        .map(|e| e.id())
        .collect();
    let epoch = world.epoch();
    for index in [0, 2, 257, 513, 515] {
        world.insert(ids[index], A(index as i32)).unwrap();
    }
    for index in [1, 2, 256, 514, 515] {
        world.insert(ids[index], B(index as i32)).unwrap();
    }
    let a = Modified::<Read<A>>::new(epoch);
    let b = Modified::<Read<B>>::new(epoch);
    let expected = [
        (Some(0), None),
        (None, Some(1)),
        (Some(2), Some(2)),
        (None, Some(256)),
        (Some(257), None),
        (Some(513), None),
        (None, Some(514)),
        (Some(515), Some(515)),
    ];
    let actual: Vec<_> = world
        .view_with(Or::from_tuple((a, b)))
        .into_iter()
        .map(|(a, b)| (a.map(|a| a.0), b.map(|b| b.0)))
        .collect();
    assert_eq!(actual, expected);
    let actual: Vec<_> = world
        .view_with(Xor::from_tuple((a, b)))
        .into_iter()
        .map(|(a, b)| (a.map(|a| a.0), b.map(|b| b.0)))
        .collect();
    let expected: Vec<_> = expected
        .into_iter()
        .filter(|(a, b)| a.is_none() || b.is_none())
        .collect();
    assert_eq!(actual, expected);
    let actual: Vec<_> = world
        .view_with(And::from_tuple((a, b)))
        .into_iter()
        .map(|(a, b)| (a.unwrap().0, b.unwrap().0))
        .collect();
    assert_eq!(actual, [(2, 2), (515, 515)]);
}

#[test]
fn boolean_modified_arms_skip_late_rejected_and_missing_components() {
    let mut world = World::new();
    let both = world.spawn((A(1), B(2))).id();
    let only_a = world.spawn_one(A(3)).id();
    let epoch = world.epoch();
    world.insert(both, A(10)).unwrap();
    world.insert(only_a, A(30)).unwrap();
    let a = Modified::<Read<A>>::new(epoch);
    let b = Modified::<Read<B>>::new(epoch);
    let mut values: Vec<_> = world
        .view_with(Or::from_tuple((a, b)))
        .into_iter()
        .map(|(a, b)| {
            assert!(b.is_none());
            a.unwrap().0
        })
        .collect();
    values.sort_unstable();
    assert_eq!(values, [10, 30]);
    assert_eq!(
        world.view_with(Xor::from_tuple((a, b))).into_iter().count(),
        2
    );
}

#[test]
fn static_xor_batched_keeps_exactly_one_archetype_arm() {
    let (world, _) = combinations();
    let view = world.view::<Xor<(&A, &B)>>();
    let mut values = Vec::new();
    for (a, b) in view.iter_batched(17) {
        assert_ne!(a.is_some(), b.is_some());
        if let Some(a) = a {
            values.extend(a.iter().map(|a| a.0));
        }
        if let Some(b) = b {
            values.extend(b.iter().map(|b| b.0));
        }
    }
    values.sort_unstable();
    assert_eq!(values, [1, 2]);
}

#[test]
fn partially_consumed_mutable_batches_only_touch_fetched_entity_range() {
    for size in [7, 256, 300] {
        let mut world = World::new();
        world.spawn_batch((0..600).map(|i| (A(i),))).spawn_all();
        let epoch = world.epoch();
        {
            let mut view = world.view::<&mut A>();
            let mut iter = view.iter_batched_mut(size);
            let batch = iter.next().unwrap();
            assert_eq!(batch.len(), size as usize);
        }
        assert_eq!(changed(&world, epoch), (0..size as i32).collect::<Vec<_>>());
    }
}

#[test]
fn negated_modified_filter_includes_unmodified_and_missing_components() {
    let mut world = World::new();
    let ids: Vec<_> = world
        .spawn_batch((0..770).map(|i| (A(i),)))
        .map(|e| e.id())
        .collect();
    let mut expected = ids.clone();
    expected.push(world.spawn_one(B(9)).id());
    expected.push(world.spawn((A(800), B(800))).id());
    let epoch = world.epoch();
    for index in [0, 257, 513] {
        world.insert(ids[index], A(1000)).unwrap();
        expected.retain(|&id| id != ids[index]);
    }
    let mut actual: Vec<_> = world
        .view::<Entities>()
        .filter(Not(Modified::<Read<A>>::new(epoch)))
        .into_iter()
        .map(|e| e.id())
        .collect();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}
