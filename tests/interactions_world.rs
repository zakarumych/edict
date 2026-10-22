use std::{
    cell::Cell,
    collections::BTreeSet,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use edict::{
    EntityError,
    action::{ActionBuffer, LocalActionBuffer, LocalActionEncoder},
    bundle::EntityBuilder,
    component::Component,
    entity::EntityId,
    epoch::EpochId,
    query::{Alt, Entities, Modified, Or, Read, With},
    relation::{ChildOf, Related, Relates, Relation},
    world::World,
};

#[derive(Component)]
struct Key(usize);
#[derive(Component, Debug, PartialEq)]
struct Value(i64);
#[derive(Component)]
struct Selected;
#[derive(Component)]
struct Archived;

struct Audit(usize, Arc<AtomicUsize>);
impl Component for Audit {
    fn on_drop(&mut self, _: EntityId, mut encoder: LocalActionEncoder) {
        let key = self.0;
        encoder.closure(move |world| world.expect_resource_mut::<Vec<usize>>().push(key));
    }
}
impl Drop for Audit {
    fn drop(&mut self) {
        self.1.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy)]
struct Link;
#[derive(Default)]
struct RelationDrops(usize);
impl Relation for Link {
    fn on_target_drop(origins: &[(EntityId, Self)], _: EntityId, mut encoder: LocalActionEncoder) {
        let removed = origins.len();
        encoder.closure(move |world| world.with_default_resource::<RelationDrops>().0 += removed);
    }
}

fn targets<R: Relation>(world: &mut World, entity: EntityId) -> BTreeSet<EntityId> {
    world
        .get::<Relates<With<R>>>(entity)
        .map(|iter| iter.map(|e| e.id()).collect())
        .unwrap_or_default()
}
fn incoming<R: Relation>(world: &mut World, entity: EntityId) -> BTreeSet<EntityId> {
    world
        .get::<Related<With<R>>>(entity)
        .map(|iter| iter.map(|e| e.id()).collect())
        .unwrap_or_default()
}
fn changed(world: &World, epoch: EpochId) -> BTreeSet<EntityId> {
    world
        .view::<Entities>()
        .extend(Modified::<Read<Value>>::new(epoch))
        .into_iter()
        .map(|(id, _)| id.id())
        .collect()
}

#[test]
fn reserved_dynamic_entities_reparent_before_cascade_and_keep_change_history() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut world = World::new();
    world.insert_resource(Vec::<usize>::new());
    let old_parent = world.spawn_empty().id();
    let new_parent = world.spawn_empty().id();
    let observer = world.spawn_one(Key(99)).id();
    let mut actions = ActionBuffer::new();
    let (child, grandchild);
    {
        let mut encoder = actions.encoder(&world);
        child = encoder
            .spawn(
                EntityBuilder::new()
                    .with(Key(1))
                    .with(Value(10))
                    .with(Audit(1, drops.clone())),
            )
            .id();
        grandchild = encoder
            .spawn((Key(2), Value(20), Audit(2, drops.clone())))
            .id();
        encoder.with(child, || -> Value {
            panic!("existing value must suppress deferred constructor")
        });
        encoder.with_bundle(child, (Value(999), Selected));
        encoder.insert_relation(child, ChildOf, old_parent);
        encoder.insert_relation(grandchild, ChildOf, child);
        encoder.insert_relation(observer, Link, grandchild);
    }
    assert!(world.is_alive(child) && world.is_alive(grandchild));
    assert_eq!(world.get::<&Value>(child), Err(EntityError::Mismatch));
    assert_eq!(world.view::<&Value>().into_iter().count(), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(actions.execute(&mut world));
    assert_eq!(world.get::<&Value>(child).unwrap().0, 10);
    assert_eq!(
        incoming::<ChildOf>(&mut world, old_parent),
        BTreeSet::from([child])
    );
    let epoch = world.epoch();
    {
        let mut encoder = actions.encoder(&world);
        encoder.insert_relation(child, ChildOf, new_parent);
        encoder.insert(grandchild, Value(21));
        encoder.with_bundle(grandchild, (Value(-1), Archived));
        encoder.despawn(old_parent);
    }
    assert_eq!(
        targets::<ChildOf>(&mut world, child),
        BTreeSet::from([old_parent])
    );
    assert_eq!(world.get::<&Value>(grandchild).unwrap().0, 20);
    assert!(changed(&world, epoch).is_empty());
    actions.execute(&mut world);
    assert!(!world.is_alive(old_parent));
    assert!(world.is_alive(child) && world.is_alive(grandchild));
    assert_eq!(
        targets::<ChildOf>(&mut world, child),
        BTreeSet::from([new_parent])
    );
    assert_eq!(changed(&world, epoch), BTreeSet::from([grandchild]));
    let matched: BTreeSet<_> = world
        .view::<(
            Entities,
            &Key,
            Or<(With<Selected>, With<Archived>)>,
            Option<&Value>,
        )>()
        .into_iter()
        .map(|(id, key, _, value)| (id.id(), key.0, value.unwrap().0))
        .collect();
    assert_eq!(
        matched,
        BTreeSet::from([(child, 1, 10), (grandchild, 2, 21)])
    );
    assert_eq!(
        targets::<Link>(&mut world, observer),
        BTreeSet::from([grandchild])
    );
    actions.encoder(&world).despawn(new_parent);
    assert!(world.is_alive(child));
    actions.execute(&mut world);
    assert!(!world.is_alive(child) && !world.is_alive(grandchild));
    assert!(world.is_alive(observer));
    assert!(targets::<Link>(&mut world, observer).is_empty());
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    let mut hooks = world.clone_resource::<Vec<usize>>();
    hooks.sort_unstable();
    assert_eq!(hooks, [1, 2]);
    assert_eq!(world.expect_resource::<RelationDrops>().0, 1);
    assert!(changed(&world, epoch).is_empty());
}

#[test]
fn threaded_commands_archetype_moves_and_selective_mutation_match_reference_snapshot() {
    const N: usize = 641;
    let drops = Arc::new(AtomicUsize::new(0));
    let mut world = World::new();
    world.insert_resource(Vec::<usize>::new());
    world.ensure_external_registered::<String>();
    let hub = world.spawn_empty().id();
    let backup = world.spawn_empty().id();
    let ids: Vec<_> = world
        .spawn_batch((0..N).map(|i| (Key(i), Value(i as i64), Audit(i, drops.clone()))))
        .map(|entity| entity.id())
        .collect();
    for (i, &id) in ids.iter().enumerate() {
        if i % 3 == 0 {
            world.insert(id, Selected).unwrap();
        }
        world.insert_relation(id, Link, hub).unwrap();
        if i % 5 == 0 {
            world.insert_relation(id, Link, backup).unwrap();
        }
    }
    let baseline = world.epoch();
    let sender = world.new_action_sender();
    let threaded_ids = ids.clone();
    std::thread::spawn(move || {
        for (i, id) in threaded_ids.into_iter().enumerate() {
            if i % 2 == 0 {
                sender.insert(id, Selected);
            } else {
                sender.drop::<Selected>(id);
            }
            if i % 7 == 0 {
                sender.insert(id, Value(-(i as i64)));
            }
            if i % 5 == 0 {
                sender.insert_external(id, format!("entity-{i}"));
            }
            if i % 11 == 0 {
                sender.despawn(id);
            }
        }
    })
    .join()
    .unwrap();
    assert_eq!(world.view::<&Value>().into_iter().count(), N);
    assert_eq!(
        world.view::<&Selected>().into_iter().count(),
        (0..N).filter(|i| i % 3 == 0).count()
    );
    assert!(changed(&world, baseline).is_empty());
    assert!(world.expect_resource::<Vec<usize>>().is_empty());
    world.execute_received_actions();
    let survivors: BTreeSet<_> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 11 != 0)
        .map(|(_, &id)| id)
        .collect();
    assert_eq!(incoming::<Link>(&mut world, hub), survivors);
    let expected_changed: BTreeSet<_> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 11 != 0 && i % 7 == 0)
        .map(|(_, &id)| id)
        .collect();
    assert_eq!(changed(&world, baseline), expected_changed);
    for (id, key, value, selected, name) in
        world.view::<(Entities, &Key, &Value, Option<&Selected>, Option<&String>)>()
    {
        let i = key.0;
        assert!(survivors.contains(&id.id()));
        assert_eq!(value.0, if i % 7 == 0 { -(i as i64) } else { i as i64 });
        assert_eq!(selected.is_some(), i % 2 == 0);
        assert_eq!(
            name.map(String::as_str),
            (i % 5 == 0).then(|| format!("entity-{i}")).as_deref()
        );
    }
    assert_eq!(drops.load(Ordering::SeqCst), N - survivors.len());
    let mutation_epoch = world.epoch();
    for (key, mut value) in world.view::<(&Key, Alt<Value>)>() {
        if key.0 % 13 == 0 {
            value.0 += 10000;
        }
    }
    {
        let mut view = world.view::<(&Key, &mut Value)>().with::<Selected>();
        for (keys, values) in view.iter_batched_mut(37) {
            assert_eq!(keys.len(), values.len());
            for (key, value) in keys.iter().zip(values.iter_mut()) {
                assert_eq!(key.0 % 2, 0);
                value.0 += 100;
            }
        }
    }
    assert_eq!(
        changed(&world, mutation_epoch),
        ids.iter()
            .enumerate()
            .filter(|(i, _)| i % 11 != 0 && (i % 13 == 0 || i % 2 == 0))
            .map(|(_, &id)| id)
            .collect()
    );
    for (key, value) in world.view::<(&Key, &Value)>() {
        let i = key.0;
        let initial = if i % 7 == 0 { -(i as i64) } else { i as i64 };
        assert_eq!(
            value.0,
            initial + if i % 13 == 0 { 10000 } else { 0 } + if i % 2 == 0 { 100 } else { 0 }
        );
    }
    world.despawn(hub).unwrap();
    assert_eq!(world.expect_resource::<RelationDrops>().0, survivors.len());
    for (i, &id) in ids.iter().enumerate().filter(|(i, _)| i % 11 != 0) {
        assert!(world.is_alive(id));
        assert_eq!(
            targets::<Link>(&mut world, id),
            if i % 5 == 0 {
                BTreeSet::from([backup])
            } else {
                BTreeSet::new()
            }
        );
    }
    assert_eq!(
        incoming::<Link>(&mut world, backup),
        ids.iter()
            .enumerate()
            .filter(|(i, _)| i % 11 != 0 && i % 5 == 0)
            .map(|(_, &id)| id)
            .collect()
    );
    world.despawn_batch(survivors);
    assert!(incoming::<Link>(&mut world, backup).is_empty());
    assert_eq!(drops.load(Ordering::SeqCst), N);
    let mut hooks = world.clone_resource::<Vec<usize>>();
    hooks.sort_unstable();
    assert_eq!(hooks, (0..N).collect::<Vec<_>>());
}

struct ArchiveDestination(EntityId);
struct ArchiveOnDrop {
    key: usize,
    completed: Rc<Cell<usize>>,
    drops: Arc<AtomicUsize>,
}
impl Component for ArchiveOnDrop {
    fn on_drop(&mut self, _: EntityId, mut encoder: LocalActionEncoder) {
        let key = self.key;
        let completed = self.completed.clone();
        let drops = self.drops.clone();
        encoder.closure(move |world| {
            let destination = world.expect_resource::<ArchiveDestination>().0;
            let id = world
                .spawn(
                    EntityBuilder::new()
                        .with(Key(key + 1000))
                        .with(Value(key as i64))
                        .with(Archived)
                        .with(Audit(key + 1000, drops)),
                )
                .id();
            world.insert_external(id, format!("archive-{key}")).unwrap();
            world.insert_relation(id, Link, destination).unwrap();
            completed.set(completed.get() + 1);
        });
    }
}

#[test]
fn cascade_hooks_build_external_archives_and_local_actions_preserve_ownership() {
    let drops = Arc::new(AtomicUsize::new(0));
    let completed = Rc::new(Cell::new(0));
    let mut world = World::new();
    world.insert_resource(Vec::<usize>::new());
    world.ensure_external_registered::<String>();
    let stage = world.spawn_empty().id();
    let archive = world.spawn_empty().id();
    world.insert_resource(ArchiveDestination(archive));
    let mut originals = Vec::new();
    for key in 0..16 {
        let id = world
            .spawn((
                Key(key),
                Value(-(key as i64)),
                Audit(key, drops.clone()),
                ArchiveOnDrop {
                    key,
                    completed: completed.clone(),
                    drops: drops.clone(),
                },
            ))
            .id();
        world.insert_external(id, format!("source-{key}")).unwrap();
        world.insert_relation(id, ChildOf, stage).unwrap();
        originals.push(id);
    }
    let control = world.spawn((Key(999), Value(999))).id();
    let epoch = world.epoch();
    let mut actions = LocalActionBuffer::new();
    {
        let mut encoder = actions.encoder(&world);
        encoder.closure({
            let completed = completed.clone();
            move |_| assert_eq!(completed.get(), 0)
        });
        encoder.despawn(stage);
        encoder.closure({
            let completed = completed.clone();
            move |_| assert_eq!(completed.get(), 16)
        });
    }
    assert_eq!(completed.get(), 0);
    assert_eq!(incoming::<ChildOf>(&mut world, stage).len(), 16);
    assert!(
        world
            .new_view()
            .with::<Archived>()
            .into_iter()
            .next()
            .is_none()
    );
    actions.execute(&mut world);
    assert_eq!(completed.get(), 16);
    assert!(originals.iter().all(|&id| !world.is_alive(id)));
    assert_eq!(drops.load(Ordering::SeqCst), 16);
    let archived = incoming::<Link>(&mut world, archive);
    assert_eq!(archived.len(), 16);
    assert_eq!(changed(&world, epoch), archived);
    for (key, value, name) in world.view::<(&Key, &Value, &String)>().with::<Archived>() {
        assert_eq!(value.0, key.0 as i64 - 1000);
        assert_eq!(name, &format!("archive-{}", key.0 - 1000));
    }
    assert_eq!(world.get::<&Value>(control).unwrap().0, 999);
    assert!(!changed(&world, epoch).contains(&control));
    world
        .local()
        .defer(move |world| world.despawn_batch(archived));
    assert_eq!(drops.load(Ordering::SeqCst), 16);
    world.run_deferred();
    assert_eq!(drops.load(Ordering::SeqCst), 32);
    assert_eq!(completed.get(), 16);
    assert!(incoming::<Link>(&mut world, archive).is_empty());
    assert_eq!(Rc::strong_count(&completed), 1);
    let hooks = world.clone_resource::<Vec<usize>>();
    let unique: BTreeSet<_> = hooks.iter().copied().collect();
    assert_eq!(hooks.len(), 32);
    assert_eq!(unique, (0..16).chain(1000..1016).collect());
    assert!(world.is_alive(control) && world.is_alive(archive));
}
