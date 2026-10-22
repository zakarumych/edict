use std::sync::{Arc, Mutex};

use edict::{
    EntityError, NoSuchEntity,
    action::LocalActionEncoder,
    bundle::EntityBuilder,
    component::Component,
    entity::{EntityId, OneRangeAllocator},
    query::Entities,
    world::{World, WorldBuilder},
};

#[derive(Debug, PartialEq, Eq)]
struct A(i32);
impl Component for A {}
#[derive(Debug, PartialEq, Eq)]
struct B(i32);
impl Component for B {}
struct C;
impl Component for C {}

fn count(world: &World) -> usize {
    world.view::<Entities>().iter().count()
}

fn isolated(test: &str, body: impl FnOnce()) {
    const CHILD_TEST: &str = "EDICT_WORLD_LIFECYCLE_CHILD_TEST";
    if std::env::var(CHILD_TEST).as_deref() == Ok(test) {
        body();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(CHILD_TEST, test)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated test {test} exited with {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn empty_entities_have_identity_without_components() {
    let mut world = World::new();
    let a = world.spawn_empty().id();
    let b = world.spawn(()).id();
    assert_ne!(a, b);
    assert!(world.is_alive(a) && world.is_alive(b));
    assert_eq!(world.try_has_component::<A>(a), Ok(false));
    assert_eq!(world.get::<&A>(a), Err(EntityError::Mismatch));
    assert_eq!(count(&world), 2);
}

#[test]
fn allocation_is_alive_and_visible_before_materialization() {
    let mut world = World::new();
    let ids: Vec<_> = (0..48).map(|_| world.allocate().id()).collect();
    for &id in &ids {
        assert!(world.is_alive(id));
        assert_eq!(world.get::<Entities>(id).unwrap().id(), id);
        assert_eq!(world.get::<&A>(id), Err(EntityError::Mismatch));
    }
    world.spawn_one(A(7));
    for id in ids {
        world.insert(id, B(9)).unwrap();
        assert_eq!(world.get::<&B>(id).unwrap().0, 9);
    }
    assert_eq!(count(&world), 49);
}

#[test]
fn insertion_materializes_reserved_entity() {
    let mut world = World::new();
    let first = world.allocate().id();
    let second = world.allocate().id();
    world.insert(second, A(7)).unwrap();
    assert!(world.is_alive(first));
    assert_eq!(world.get::<&A>(second).unwrap().0, 7);
    assert_eq!(count(&world), 2);
}

#[test]
fn bundle_insertion_materializes_reserved_entity() {
    let mut world = World::new();
    let id = world.allocate().id();
    world.insert_bundle(id, (A(3), B(4))).unwrap();
    assert_eq!(world.get::<&A>(id).unwrap().0, 3);
    assert_eq!(world.get::<&B>(id).unwrap().0, 4);
}

#[test]
fn reserved_entity_can_be_despawned() {
    let mut world = World::new();
    let id = world.allocate().id();
    world.despawn(id).unwrap();
    assert!(!world.is_alive(id));
    assert_eq!(count(&world), 0);
}

#[test]
fn batch_drop_spawns_only_consumed_items() {
    let mut world = World::new();
    let first = {
        let mut batch = world.spawn_batch((0..20).map(|n| (A(n),)));
        assert_eq!(batch.len(), 20);
        let first = batch.next().unwrap().id();
        assert_eq!(batch.size_hint(), (19, Some(19)));
        first
    };
    assert_eq!(count(&world), 1);
    assert_eq!(world.get::<&A>(first).unwrap().0, 0);
}

#[test]
fn batch_nth_skips_without_spawning() {
    let mut world = World::new();
    let ids = {
        let mut batch = world.spawn_batch((0..10).map(|n| (A(n),)));
        let a = batch.nth(3).unwrap().id();
        let b = batch.next().unwrap().id();
        assert!(batch.nth(99).is_none());
        assert!(batch.next().is_none());
        [a, b]
    };
    assert_eq!(count(&world), 2);
    assert_eq!(world.get::<&A>(ids[0]).unwrap().0, 3);
    assert_eq!(world.get::<&A>(ids[1]).unwrap().0, 4);
}

#[test]
fn double_ended_batch_obeys_source_order_and_remaining_length() {
    let mut world = World::new();
    let ids = {
        let mut batch = world.spawn_batch((0..8).map(|n| (A(n),)));
        let back = batch.next_back().unwrap().id();
        let middle = batch.nth_back(2).unwrap().id();
        let front = batch.next().unwrap().id();
        assert_eq!(batch.len(), 3);
        [back, middle, front]
    };
    for (id, value) in ids.into_iter().zip([7, 4, 0]) {
        assert_eq!(world.get::<&A>(id).unwrap().0, value);
    }
    assert_eq!(count(&world), 3);
}

#[test]
fn batch_spawn_all_drains_remaining_items_once() {
    let mut world = World::new();
    {
        let mut batch = world.spawn_batch((0..1300).map(|n| (A(n), B(-n))));
        batch.next().unwrap();
        batch.spawn_all();
        batch.spawn_all();
        assert_eq!(batch.len(), 0);
        assert!(batch.next_back().is_none());
    }
    assert_eq!(count(&world), 1300);
    assert_eq!(
        world.view::<&A>().iter().map(|a| a.0).sum::<i32>(),
        1299 * 1300 / 2
    );
    for (a, b) in world.view::<(&A, &B)>() {
        assert_eq!(a.0, -b.0);
    }
}

#[test]
fn batch_collect_and_reverse_fold_preserve_values() {
    let mut world = World::new();
    let ids: Vec<_> = world
        .spawn_batch((0..128).map(|n| (A(n),)))
        .map(|e| e.id())
        .collect();
    for (n, id) in ids.into_iter().enumerate() {
        assert_eq!(world.get::<&A>(id).unwrap().0, n as i32);
    }
    let reversed = world
        .spawn_batch((0..64).map(|n| (B(n),)))
        .rfold(Vec::new(), |mut ids, e| {
            ids.push(e.id());
            ids
        });
    for (n, id) in reversed.into_iter().enumerate() {
        assert_eq!(world.get::<&B>(id).unwrap().0, 63 - n as i32);
    }
    assert_eq!(count(&world), 192);
}

#[test]
fn insert_replaces_while_with_is_lazy() {
    let mut world = World::new();
    let id = world.spawn_one(A(1)).id();
    world
        .with(id, || -> A {
            panic!("existing component must skip constructor")
        })
        .unwrap();
    assert_eq!(world.get::<&A>(id).unwrap().0, 1);
    world.insert(id, A(2)).unwrap();
    let mut calls = 0;
    world
        .with(id, || {
            calls += 1;
            B(3)
        })
        .unwrap();
    world
        .with(id, || {
            calls += 1;
            B(4)
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(world.get::<&B>(id).unwrap().0, 3);
    assert_eq!(world.get::<&A>(id).unwrap().0, 2);
}

#[test]
fn bundle_overlap_replaces_or_preserves_as_requested() {
    let mut world = World::new();
    let id = world.spawn((A(1), C)).id();
    world.with_bundle(id, (A(2), B(3))).unwrap();
    assert_eq!(world.get::<&A>(id).unwrap().0, 1);
    assert_eq!(world.get::<&B>(id).unwrap().0, 3);
    world.insert_bundle(id, (B(5), A(4))).unwrap();
    assert_eq!(world.get::<&A>(id).unwrap().0, 4);
    assert_eq!(world.get::<&B>(id).unwrap().0, 5);
    assert!(world.try_has_component::<C>(id).unwrap());
    world.insert_bundle(id, ()).unwrap();
    world.with_bundle(id, ()).unwrap();
    assert_eq!(count(&world), 1);
}

#[test]
fn removing_absent_component_preserves_entity_and_other_values() {
    let mut world = World::new();
    let id = world.spawn_one(A(11)).id();
    assert_eq!(world.remove::<B>(id).unwrap().0, None);
    assert_eq!(world.drop::<B>(id), Err(EntityError::Mismatch));
    assert_eq!(world.remove::<A>(id).unwrap().0, Some(A(11)));
    assert_eq!(world.remove::<A>(id).unwrap().0, None);
    assert!(world.is_alive(id));
    assert_eq!(count(&world), 1);
}

#[test]
fn drop_bundle_removes_intersection_and_allows_unregistered_types() {
    struct Unregistered;
    let mut world = World::new();
    let id = world.spawn((A(1), B(2), C)).id();
    world.drop_bundle::<(A, Unregistered)>(id).unwrap();
    assert!(!world.try_has_component::<A>(id).unwrap());
    assert_eq!(world.get::<&B>(id).unwrap().0, 2);
    world.drop_bundle::<(B, C)>(id).unwrap();
    world.drop_bundle::<()>(id).unwrap();
    assert!(world.is_alive(id));
    assert_eq!(world.get::<&B>(id), Err(EntityError::Mismatch));
}

#[test]
fn stale_ids_reject_mutations_and_skip_lazy_constructors() {
    let mut world = World::new();
    let id = world.spawn_one(A(1)).id();
    world.despawn(id).unwrap();
    for _ in 0..256 {
        assert_ne!(world.spawn_empty().id(), id);
    }
    assert_eq!(world.get::<&A>(id), Err(EntityError::NoSuchEntity));
    assert_eq!(world.try_has_component::<A>(id), Err(NoSuchEntity));
    assert_eq!(world.insert(id, A(2)), Err(NoSuchEntity));
    assert!(
        world
            .with(id, || -> A { panic!("dead entity must skip constructor") })
            .is_err()
    );
    assert!(world.remove::<A>(id).is_err());
    assert!(world.insert_bundle(id, (B(3),)).is_err());
    assert!(world.drop_bundle::<(A, B)>(id).is_err());
    assert_eq!(world.despawn(id), Err(NoSuchEntity));
}

#[test]
fn swap_removal_keeps_every_surviving_entity_location_valid() {
    let mut world = World::new();
    let ids: Vec<_> = world
        .spawn_batch((0..257).map(|n| (A(n), B(-n))))
        .map(|e| e.id())
        .collect();
    for index in (0..ids.len()).step_by(3) {
        world.despawn(ids[index]).unwrap();
    }
    for (index, &id) in ids.iter().enumerate() {
        if index % 3 == 0 {
            assert!(!world.is_alive(id));
            continue;
        }
        assert_eq!(world.get::<&A>(id).unwrap().0, index as i32);
        world.insert(id, C).unwrap();
        assert_eq!(world.remove::<B>(id).unwrap().0, Some(B(-(index as i32))));
        assert_eq!(world.get::<&A>(id).unwrap().0, index as i32);
    }
    assert_eq!(count(&world), 171);
}

#[test]
fn batch_despawn_and_drop_tolerate_duplicates_and_missing_entities() {
    let mut world = World::new();
    let a = world.spawn((A(1), B(2))).id();
    let b = world.spawn_one(B(3)).id();
    let dead = world.spawn_empty().id();
    world.despawn(dead).unwrap();
    world.drop_batch::<A>([a, a, b, dead]);
    assert_eq!(world.get::<&A>(a), Err(EntityError::Mismatch));
    assert_eq!(world.get::<&B>(a).unwrap().0, 2);
    world.despawn_batch([a, dead, a, b, b]);
    assert_eq!(count(&world), 0);
}

#[test]
fn erased_drop_only_removes_requested_type() {
    let mut world = World::new();
    let a = world.spawn((A(1), B(2))).id();
    let b = world.spawn((A(3), B(4))).id();
    world.drop_erased(a, std::any::TypeId::of::<A>()).unwrap();
    world.drop_erased_batch([a, b], std::any::TypeId::of::<B>());
    assert_eq!(world.get::<&A>(a), Err(EntityError::Mismatch));
    assert_eq!(world.get::<&A>(b).unwrap().0, 3);
    assert_eq!(world.view::<&B>().iter().count(), 0);
    assert_eq!(count(&world), 2);
}

#[test]
fn external_components_support_registration_insert_with_and_bundles() {
    let mut world = World::new();
    world.ensure_external_registered::<String>();
    world.ensure_external_registered::<u64>();
    let id = world.spawn_external((String::from("first"), 3u64)).id();
    world.insert_external(id, String::from("second")).unwrap();
    world
        .with_external(id, || -> String { panic!("present external component") })
        .unwrap();
    world
        .with_external_bundle(id, (String::from("ignored"), 4u64))
        .unwrap();
    assert_eq!(world.get::<&String>(id).unwrap(), "second");
    assert_eq!(*world.get::<&u64>(id).unwrap(), 3);
    world
        .insert_external_bundle(id, (9u64, String::from("third")))
        .unwrap();
    assert_eq!(
        world.remove::<String>(id).unwrap().0.as_deref(),
        Some("third")
    );
    assert_eq!(*world.get::<&u64>(id).unwrap(), 9);
}

#[test]
fn external_batch_preserves_values() {
    let mut world = World::new();
    world.ensure_external_registered::<String>();
    let ids: Vec<_> = world
        .spawn_batch_external((0..80).map(|n| (n.to_string(),)))
        .map(|e| e.id())
        .collect();
    for (n, id) in ids.into_iter().enumerate() {
        assert_eq!(world.get::<&String>(id).unwrap(), &n.to_string());
    }
}

#[test]
#[should_panic(expected = "not registered")]
fn unregistered_external_component_is_rejected() {
    World::new().spawn_one_external(String::new());
}

#[test]
#[should_panic(expected = "not valid")]
fn duplicate_tuple_component_types_are_rejected() {
    World::new().spawn((A(1), A(2)));
}

#[test]
fn dynamic_builder_replaces_duplicate_types_and_transfers_values() {
    isolated(
        "dynamic_builder_replaces_duplicate_types_and_transfers_values",
        || {
            let mut builder = EntityBuilder::new();
            assert!(builder.is_empty());
            builder.add(A(1)).add(B(2)).add(A(3));
            builder.get_mut::<B>().unwrap().0 = 4;
            assert_eq!(builder.get::<A>(), Some(&A(3)));
            assert!(builder.get::<C>().is_none());
            assert_eq!(builder.component_types().count(), 2);
            let mut world = World::new();
            let id = world.spawn(builder).id();
            assert_eq!(world.get::<&A>(id).unwrap().0, 3);
            assert_eq!(world.get::<&B>(id).unwrap().0, 4);
            world
                .insert_bundle(id, EntityBuilder::new().with(A(5)).with(C))
                .unwrap();
            assert_eq!(world.get::<&A>(id).unwrap().0, 5);
            assert_eq!(world.get::<&B>(id).unwrap().0, 4);
            assert!(world.try_has_component::<C>(id).unwrap());
            let builder = EntityBuilder::new()
                .with(C)
                .with(Aligned([99; 33]))
                .with(A(7));
            let id = world.spawn(builder).id();
            assert_eq!(world.get::<&Aligned>(id).unwrap().0, [99; 33]);
        },
    );
}

#[test]
fn entity_ref_stays_usable_after_component_transitions() {
    let mut world = World::new();
    let mut entity = world.spawn_one(A(1));
    let id = entity.id();
    entity.with_bundle((B(2), C));
    assert_eq!(entity.remove::<A>(), Some(A(1)));
    entity.with(|| A(3)).0 += 1;
    let entity = entity.insert(B(5)).unwrap();
    assert_eq!(entity.get::<&A>().unwrap().0, 4);
    assert_eq!(entity.get::<&B>().unwrap().0, 5);
    assert_eq!(entity.id(), id);
    entity.despawn();
    assert!(!world.is_alive(id));
}

#[test]
fn explicit_ids_and_spawn_or_insert_preserve_existing_components() {
    let mut world = WorldBuilder::new()
        .with_id_range_allocator(Box::new(OneRangeAllocator::client()))
        .build();
    let id = EntityId::from_bits(1 << 60).unwrap();
    world.spawn_at(id, (A(1), C));
    world.spawn_or_insert(id, (A(2), B(3)));
    assert_eq!(count(&world), 1);
    assert_eq!(world.get::<&A>(id).unwrap().0, 2);
    assert_eq!(world.get::<&B>(id).unwrap().0, 3);
    assert!(world.try_has_component::<C>(id).unwrap());
    let other = EntityId::from_bits((1 << 60) + 1).unwrap();
    world.spawn_or_insert(other, (B(4),));
    assert_eq!(count(&world), 2);
    assert_ne!(world.spawn_empty().id(), id);
}

#[repr(align(256))]
struct Aligned([u64; 33]);
impl Component for Aligned {}

#[repr(align(256))]
struct AlignedZst;
impl Component for AlignedZst {}

#[test]
fn empty_and_aligned_zero_sized_builders_are_valid() {
    isolated("empty_and_aligned_zero_sized_builders_are_valid", || {
        drop(EntityBuilder::new());
        let mut builder = EntityBuilder::new().with(AlignedZst);
        let value = builder.get_mut::<AlignedZst>().unwrap();
        assert_eq!((value as *mut AlignedZst as usize) % 256, 0);
        let mut world = World::new();
        let id = world.spawn(builder).id();
        assert!(world.try_has_component::<AlignedZst>(id).unwrap());
        world.spawn(EntityBuilder::new());
        assert_eq!(count(&world), 2);
    });
}

#[test]
fn over_aligned_and_zero_sized_components_survive_growth_and_moves() {
    let mut world = World::new();
    let ids: Vec<_> = (0..150)
        .map(|n| world.spawn((Aligned([n; 33]), C)).id())
        .collect();
    for (n, &id) in ids.iter().enumerate() {
        world.insert(id, A(n as i32)).unwrap();
        world.drop::<C>(id).unwrap();
        let aligned = world.get::<&Aligned>(id).unwrap();
        assert_eq!((aligned as *const Aligned as usize) % 256, 0);
        assert_eq!(aligned.0, [n as u64; 33]);
        world.insert(id, C).unwrap();
    }
    assert_eq!(world.view::<&C>().iter().count(), 150);
}

#[derive(Clone)]
struct Tracked {
    value: i32,
    events: Arc<Mutex<Vec<(&'static str, i32)>>>,
    drop_hook_on_replace: bool,
}
impl Component for Tracked {
    fn on_drop(&mut self, _: EntityId, _: LocalActionEncoder) {
        self.events.lock().unwrap().push(("hook", self.value));
    }
    fn on_replace(&mut self, _: &Self, _: EntityId, _: LocalActionEncoder) -> bool {
        self.events.lock().unwrap().push(("replace", self.value));
        self.drop_hook_on_replace
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        self.events.lock().unwrap().push(("rust", self.value));
    }
}
fn tracked(value: i32, events: &Arc<Mutex<Vec<(&'static str, i32)>>>) -> Tracked {
    Tracked {
        value,
        events: events.clone(),
        drop_hook_on_replace: true,
    }
}

#[test]
fn remove_transfers_ownership_without_running_component_drop_hook() {
    let events = Arc::default();
    let mut world = World::new();
    let id = world.spawn((tracked(1, &events), A(2))).id();
    world.insert(id, B(3)).unwrap();
    let removed = world.remove::<Tracked>(id).unwrap().0.unwrap();
    assert!(events.lock().unwrap().is_empty());
    world.despawn(id).unwrap();
    assert!(events.lock().unwrap().is_empty());
    drop(removed);
    assert_eq!(*events.lock().unwrap(), [("rust", 1)]);
}

#[test]
fn replacement_hook_controls_component_hook_but_not_rust_destructor() {
    for hooks in [false, true] {
        let events = Arc::default();
        let mut world = World::new();
        let mut old = tracked(1, &events);
        old.drop_hook_on_replace = hooks;
        let id = world.spawn_one(old).id();
        world.insert(id, tracked(2, &events)).unwrap();
        let expected = if hooks {
            vec![("replace", 1), ("hook", 1), ("rust", 1)]
        } else {
            vec![("replace", 1), ("rust", 1)]
        };
        assert_eq!(*events.lock().unwrap(), expected);
        world.despawn(id).unwrap();
        let mut expected = expected;
        expected.extend([("hook", 2), ("rust", 2)]);
        assert_eq!(*events.lock().unwrap(), expected);
    }
}

#[test]
fn builder_replacement_drop_and_world_ownership_run_destructors_once() {
    isolated(
        "builder_replacement_drop_and_world_ownership_run_destructors_once",
        || {
            let events = Arc::default();
            {
                let mut builder = EntityBuilder::new().with(tracked(1, &events));
                builder.add(tracked(2, &events));
                assert_eq!(*events.lock().unwrap(), [("rust", 1)]);
                let mut world = World::new();
                world.spawn(builder);
                assert_eq!(*events.lock().unwrap(), [("rust", 1)]);
            }
            assert_eq!(
                events
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|&&(event, value)| event == "rust" && value == 2)
                    .count(),
                1
            );
            events.lock().unwrap().clear();
            drop(EntityBuilder::new().with(tracked(3, &events)));
            assert_eq!(*events.lock().unwrap(), [("rust", 3)]);
        },
    );
}

#[test]
fn builder_with_bundle_drops_unused_values_once_and_preserves_existing_value() {
    isolated(
        "builder_with_bundle_drops_unused_values_once_and_preserves_existing_value",
        || {
            let events = Arc::default();
            let mut world = World::new();
            let id = world.spawn_one(tracked(1, &events)).id();
            world
                .with_bundle(
                    id,
                    EntityBuilder::new().with(tracked(2, &events)).with(A(7)),
                )
                .unwrap();
            assert_eq!(*events.lock().unwrap(), [("rust", 2)]);
            assert_eq!(world.get::<&Tracked>(id).unwrap().value, 1);
            assert_eq!(world.get::<&A>(id).unwrap().0, 7);
            world.despawn(id).unwrap();
            assert_eq!(
                *events.lock().unwrap(),
                [("rust", 2), ("hook", 1), ("rust", 1)]
            );
            assert_eq!(Arc::strong_count(&events), 1);
        },
    );
}

#[test]
fn spawn_or_insert_invokes_replacement_and_drop_hooks_once() {
    let events = Arc::default();
    let mut world = World::new();
    let id = world.spawn((tracked(1, &events), A(7))).id();
    let entity = world.spawn_or_insert(id, (tracked(2, &events), B(8)));
    assert_eq!(entity.get::<&Tracked>().unwrap().value, 2);
    assert_eq!(entity.get::<&A>().unwrap().0, 7);
    assert_eq!(entity.get::<&B>().unwrap().0, 8);
    assert_eq!(
        *events.lock().unwrap(),
        [("replace", 1), ("hook", 1), ("rust", 1)]
    );
    world.despawn(id).unwrap();
    assert_eq!(Arc::strong_count(&events), 1);
}

#[test]
#[should_panic(expected = "Component replacement hook despawned entity")]
fn spawn_or_insert_does_not_return_reference_to_entity_despawned_by_hook() {
    struct DespawnOnReplace;
    impl Component for DespawnOnReplace {
        fn on_replace(&mut self, _: &Self, id: EntityId, mut encoder: LocalActionEncoder) -> bool {
            encoder.despawn(id);
            true
        }
    }
    let mut world = World::new();
    let id = world.spawn_one(DespawnOnReplace).id();
    world.spawn_or_insert(id, (DespawnOnReplace,));
}

#[test]
fn deterministic_storage_operations_match_independent_reference_model() {
    for seed in [1u64, 0xabcdef, 0x123456789] {
        let mut world = World::new();
        let mut model: Vec<(EntityId, Option<i32>, Option<i32>, bool)> = Vec::new();
        for n in 0..400 {
            let id = world.spawn((A(n), B(-n))).id();
            model.push((id, Some(n), Some(-n), true));
        }
        let mut state = seed;
        for step in 0..1800 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let op = (state >> 32) % 9;
            let value = step;
            if model.is_empty() || op == 0 {
                let id = world.spawn_one(A(value)).id();
                model.push((id, Some(value), None, true));
            } else {
                let index = (state as usize) % model.len();
                let (id, a, b, alive) = &mut model[index];
                if *alive {
                    match op {
                        1 => {
                            world.insert(*id, A(value)).unwrap();
                            *a = Some(value);
                        }
                        2 => {
                            world.insert_bundle(*id, (B(-value), A(value))).unwrap();
                            *a = Some(value);
                            *b = Some(-value);
                        }
                        3 => {
                            world.with(*id, || B(value)).unwrap();
                            b.get_or_insert(value);
                        }
                        4 => {
                            assert_eq!(world.remove::<A>(*id).unwrap().0.map(|a| a.0), a.take());
                        }
                        5 => {
                            world.drop_bundle::<(A, B)>(*id).unwrap();
                            *a = None;
                            *b = None;
                        }
                        6 => {
                            world.despawn(*id).unwrap();
                            *alive = false;
                        }
                        7 => {
                            world.with_bundle(*id, (A(value), B(-value))).unwrap();
                            a.get_or_insert(value);
                            b.get_or_insert(-value);
                        }
                        _ => {
                            if let Some(expected) = a {
                                world.get::<&mut A>(*id).unwrap().0 += 1;
                                *expected += 1;
                            }
                        }
                    }
                } else {
                    assert_eq!(world.insert(*id, A(value)), Err(NoSuchEntity));
                }
            }
            if step % 19 == 0 || step == 1799 {
                assert_eq!(count(&world), model.iter().filter(|entry| entry.3).count());
                for &(id, a, b, alive) in &model {
                    assert_eq!(world.is_alive(id), alive, "seed {seed}, step {step}");
                    if alive {
                        assert_eq!(
                            world.get::<Option<&A>>(id).unwrap().map(|a| a.0),
                            a,
                            "seed {seed}, step {step}"
                        );
                        assert_eq!(
                            world.get::<Option<&B>>(id).unwrap().map(|b| b.0),
                            b,
                            "seed {seed}, step {step}"
                        );
                    }
                }
                assert_eq!(
                    world.view::<&A>().iter().count(),
                    model
                        .iter()
                        .filter(|entry| entry.3 && entry.1.is_some())
                        .count()
                );
                assert_eq!(
                    world.view::<&B>().iter().count(),
                    model
                        .iter()
                        .filter(|entry| entry.3 && entry.2.is_some())
                        .count()
                );
            }
        }
    }
}
