use edict::{
    component::Component,
    query::{Alt, Cpy, EpochOf, Modified, With, WithEpoch},
    world::World,
};

#[derive(Clone, Copy, Component)]
struct Value(u32);
#[derive(Component)]
struct Other;

#[test]
fn with_epoch_reuse_reports_previous_entity_epoch_on_every_access() {
    let mut world = World::new();
    let a = world.spawn((Value(0),)).id();
    let b = world.spawn((Value(0),)).id();
    let initial = [
        world.get::<EpochOf<Value>>(a).unwrap(),
        world.get::<EpochOf<Value>>(b).unwrap(),
    ];
    let cutoff = world.epoch();
    let mut expected = initial;
    {
        let mut writes = world.view::<WithEpoch<&mut Value>>();
        for index in 0..600 {
            let slot = index % 2;
            let (value, previous) = writes.try_get_mut([a, b][slot]).unwrap();
            assert_eq!(previous, expected[slot]);
            value.0 += 1;
            let current = world.epoch();
            assert!(current.after(previous));
            expected[slot] = current;
        }
    }
    assert_eq!(world.get::<EpochOf<Value>>(a).unwrap(), expected[0]);
    assert_eq!(world.get::<EpochOf<Value>>(b).unwrap(), expected[1]);
    assert_eq!(
        world
            .view_with(Modified::<With<Value>>::new(cutoff))
            .iter()
            .count(),
        2
    );
}

#[test]
fn normal_write_reuse_refreshes_archetype_chunk_and_entity_epochs() {
    let mut world = World::new();
    let a = world.spawn((Value(0),)).id();
    let b = world.spawn((Value(0),)).id();
    let after_first;
    {
        let mut view = world.view::<&mut Value>();
        view.try_get_mut(a).unwrap().0 += 1;
        after_first = world.epoch();
        view.try_get_mut(b).unwrap().0 += 1;
    }
    let modified: Vec<_> = world
        .view_with(Modified::<Cpy<Value>>::new(after_first))
        .iter()
        .map(|v| v.0)
        .collect();
    assert_eq!(modified, [1]);
    assert!(!world.get::<EpochOf<Value>>(a).unwrap().after(after_first));
    assert!(world.get::<EpochOf<Value>>(b).unwrap().after(after_first));
}

#[test]
fn alt_reuse_keeps_readonly_access_invisible_and_mutation_visible() {
    let mut world = World::new();
    let id = world.spawn((Value(7),)).id();
    let cutoff = world.epoch();
    {
        let mut view = world.view::<Alt<Value>>();
        for _ in 0..600 {
            let value = view.try_get_mut(id).unwrap();
            assert_eq!(value.0, 7);
        }
    }
    assert_eq!(
        world
            .view_with(Modified::<With<Value>>::new(cutoff))
            .iter()
            .count(),
        0
    );
    {
        let mut view = world.view::<Alt<Value>>();
        for _ in 0..600 {
            view.try_get_mut(id).unwrap().0 += 1;
        }
    }
    assert_eq!(world.get::<&Value>(id).unwrap().0, 607);
    assert_eq!(
        world
            .view_with(Modified::<With<Value>>::new(cutoff))
            .iter()
            .count(),
        1
    );
}

#[test]
fn reused_readonly_tuple_option_and_epoch_queries_keep_items_valid() {
    let mut world = World::new();
    let a = world.spawn((Value(5),)).id();
    let b = world.spawn((Value(9),)).id();
    let absent = world.spawn((Other,)).id();
    let view = world.view::<(Option<&Value>, Option<Cpy<Value>>, Option<EpochOf<Value>>)>();
    let first = view.try_get(a).unwrap();
    let second = view.try_get(b).unwrap();
    assert!(view.try_get(absent).unwrap().0.is_none());
    let again = view.try_get(a).unwrap();
    assert_eq!(first.0.unwrap().0, 5);
    assert_eq!(second.0.unwrap().0, 9);
    assert_eq!(again.1.unwrap().0, 5);
    assert_eq!(first.2, again.2);
}

#[test]
fn local_views_reuse_fetches_with_fresh_cell_epochs() {
    let mut world = World::new();
    let id = world.spawn((Value(0),)).id();
    let local = world.local();
    let mut previous = local.epoch();
    let mut view = local.view::<WithEpoch<&mut Value>>();
    for _ in 0..300 {
        let (value, old) = view.try_get_mut(id).unwrap();
        assert_eq!(old, previous);
        value.0 += 1;
        assert!(local.epoch().after(previous));
        previous = local.epoch();
    }
}
