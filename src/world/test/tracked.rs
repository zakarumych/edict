use edict:: prelude::*;
use paste::paste;
use smallvec::SmallVec;

// Components for testing
#[derive(Debug)]
struct Foo();
#[derive(Debug)]
struct Bar();
#[derive(Debug)]
struct Value(u32);

/// Number of expected changes since particular Tracks
#[derive(Debug)]
struct ExpectedChanges {
    since: Tracks,
    count: usize,
}

/// List of multiple expected changes
type ExpectedChangesList = SmallVec<[ExpectedChanges; 8]>;

/// Shortcut to add expected changes to the list
fn push_expected_changes(list: &mut ExpectedChangesList, since: Tracks, count: usize) {
    list.push(ExpectedChanges { since, count });
}

/// Make the world with pre-defined initial epoch
fn make_world(initial_epoch: u64) -> World {
    World {
        epoch: initial_epoch,
        ..World::default()
    }
}

/// Spawn one entity
fn test_spawn_one(final_epoch: u64) -> (World, ExpectedChangesList) {
    let mut world = make_world(final_epoch.wrapping_sub(1));
    let mut changes_list = ExpectedChangesList::default();

    push_expected_changes(&mut changes_list, world.tracks_now(), 1);

    world.spawn((Value(1),));

    assert_eq!(world.epoch, final_epoch);
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    (world, changes_list)
}

/// Spawn multiple entities
fn test_spawn(final_epoch: u64) -> (World, ExpectedChangesList) {
    let mut world = make_world(final_epoch.wrapping_sub(4));
    let mut changes_list = ExpectedChangesList::default();

    push_expected_changes(&mut changes_list, world.tracks_now(), 2);

    world.spawn((Foo(), Bar(), Value(1)));

    assert_eq!(world.epoch, final_epoch.wrapping_sub(3));
    push_expected_changes(&mut changes_list, world.tracks_now(), 1);

    world.spawn((Foo(), Bar()));

    assert_eq!(world.epoch, final_epoch.wrapping_sub(2));
    push_expected_changes(&mut changes_list, world.tracks_now(), 1);

    world.spawn((Foo(), Bar(), Value(1)));

    assert_eq!(world.epoch, final_epoch.wrapping_sub(1));
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    world.spawn((Foo(), Bar()));

    assert_eq!(world.epoch, final_epoch);
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    (world, changes_list)
}

/// Spawn batch
fn test_spawn_batch(final_epoch: u64) -> (World, ExpectedChangesList) {
    let mut world = make_world(final_epoch.wrapping_sub(2));
    let mut changes_list = ExpectedChangesList::default();

    push_expected_changes(&mut changes_list, world.tracks_now(), 4);

    world
        .spawn_batch((0..2).into_iter().map(|n| (Foo(), Bar(), Value(n))))
        .spawn_all();

    assert_eq!(world.epoch, final_epoch.wrapping_sub(1));
    push_expected_changes(&mut changes_list, world.tracks_now(), 2);

    world
        .spawn_batch((2..4).into_iter().map(|n| (Foo(), Bar(), Value(n))))
        .spawn_all();

    assert_eq!(world.epoch, final_epoch);
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    (world, changes_list)
}

/// Modify with for_each
fn test_modify_for_each(final_epoch: u64) -> (World, ExpectedChangesList) {
    let mut world = make_world(final_epoch.wrapping_sub(3));
    let mut changes_list = ExpectedChangesList::default();

    push_expected_changes(&mut changes_list, world.tracks_now(), 2);

    world.spawn((Foo(), Value(1)));
    world.spawn((Bar(), Value(2)));

    assert_eq!(world.epoch, final_epoch.wrapping_sub(1));
    push_expected_changes(&mut changes_list, world.tracks_now(), 1);

    world.for_each_mut::<(&Bar, &mut Value), _>(|(_, v)| v.0 += 1);

    assert_eq!(world.epoch, final_epoch);
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    (world, changes_list)
}

/// Modify with query
fn test_modify_query(final_epoch: u64) -> (World, ExpectedChangesList) {
    let mut world = make_world(final_epoch.wrapping_sub(3));
    let mut changes_list = ExpectedChangesList::default();

    push_expected_changes(&mut changes_list, world.tracks_now(), 2);

    world.spawn((Foo(), Value(1)));
    world.spawn((Bar(), Value(2)));

    assert_eq!(world.epoch, final_epoch.wrapping_sub(1));
    push_expected_changes(&mut changes_list, world.tracks_now(), 1);

    world
        .query_mut::<(&Bar, &mut Value)>()
        .into_iter()
        .for_each(|(_, (_, v))| v.0 += 1);

    assert_eq!(world.epoch, final_epoch);
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    (world, changes_list)
}

/// Modify with query
fn test_modify_query_one(final_epoch: u64) -> (World, ExpectedChangesList) {
    let mut world = make_world(final_epoch.wrapping_sub(3));
    let mut changes_list = ExpectedChangesList::default();

    push_expected_changes(&mut changes_list, world.tracks_now(), 2);

    world.spawn((Foo(), Value(1)));
    let entity = world.spawn((Bar(), Value(2)));

    assert_eq!(world.epoch, final_epoch.wrapping_sub(1));
    push_expected_changes(&mut changes_list, world.tracks_now(), 1);

    world
        .query_one_mut::<(&Bar, &mut Value)>(&entity)
        .map(|(_, v)| v.0 += 1)
        .expect("Query one");

    assert_eq!(world.epoch, final_epoch);
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    (world, changes_list)
}

/// Modify with for_each with alt
fn test_modify_for_each_alt(final_epoch: u64) -> (World, ExpectedChangesList) {
    let mut world = make_world(final_epoch.wrapping_sub(3));
    let mut changes_list = ExpectedChangesList::default();

    push_expected_changes(&mut changes_list, world.tracks_now(), 10);

    world
        .spawn_batch((0..10).into_iter().map(|n| (Foo(), Value(n))))
        .spawn_all();

    assert_eq!(world.epoch, final_epoch.wrapping_sub(2));
    push_expected_changes(&mut changes_list, world.tracks_now(), 7);

    let mut count_all = 0;
    let mut count_matching = 0;
    world.for_each_mut::<(&Foo, Alt<Value>), _>(|(_, mut v)| {
        count_all += 1;
        if v.0 < 7 {
            count_matching += 1;
            v.0 += 1;
        }
    });
    assert_eq!(count_all, 10);
    assert_eq!(count_matching, 7);

    assert_eq!(world.epoch, final_epoch.wrapping_sub(1));
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    let mut count_all = 0;
    world.for_each_mut::<(&Foo, Alt<Value>), _>(|(_, mut v)| {
        count_all += 1;
        if v.0 == 100 {
            v.0 += 1;
            unreachable!()
        }
    });
    assert_eq!(count_all, 10);

    assert_eq!(world.epoch, final_epoch);
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    (world, changes_list)
}

/// Modify by query one
fn test_modify_by_query_one(final_epoch: u64) -> (World, ExpectedChangesList) {
    let mut world = make_world(final_epoch.wrapping_sub(4));
    let mut changes_list = ExpectedChangesList::default();

    push_expected_changes(&mut changes_list, world.tracks_now(), 10);

    let entities: Vec<_> = world
        .spawn_batch((0..10).into_iter().map(|n| (Foo(), Value(n))))
        .collect();

    assert_eq!(world.epoch, final_epoch.wrapping_sub(3));
    push_expected_changes(&mut changes_list, world.tracks_now(), 3);

    entities.iter().take(3).for_each(|e| {
        world
            .query_one_mut::<(&mut Value,)>(e)
            .map(|(value,)| value.0 += 1)
            .unwrap_or(());
    });

    assert_eq!(world.epoch, final_epoch);
    push_expected_changes(&mut changes_list, world.tracks_now(), 0);

    (world, changes_list)
}

/// Count and verify changed components by for_each_tracked method
fn count_for_each_tracked(world: World, changes_list: ExpectedChangesList) {
    changes_list.iter().for_each(|changes| {
        let mut count = 0;
        world.for_each_tracked::<(Modified<&Value>,), _>(&mut changes.since.clone(), |(_,)| {
            count += 1;
        });
        assert_eq!(count, changes.count);
    })
}

/// Count and verify changed components by tracked query with fold method
fn count_query_tracked_fold(world: World, changes_list: ExpectedChangesList) {
    changes_list.iter().for_each(|changes| {
        let count = world
            .query::<(Modified<&Value>,)>()
            .tracked_into_iter(&mut changes.since.clone())
            .fold(0, |count, _| count + 1);
        assert_eq!(count, changes.count);
    })
}

/// Count and verify changed components by tracked query in for loop
fn count_query_tracked_for_loop(world: World, changes_list: ExpectedChangesList) {
    changes_list.iter().for_each(|changes| {
        let mut count = 0;
        for _ in world
            .query::<(Modified<&Value>,)>()
            .tracked_into_iter(&mut changes.since.clone())
        {
            count += 1;
        }
        assert_eq!(count, changes.count);
    })
}

const HALF_OF_MAX_EPOCH: u64 = u64::MAX / 2;

macro_rules! parametrized_tests {
    ($test_fn:ident, $count_fn:ident, $final_epoch:ident) => {
        paste!{
            #[allow(non_snake_case)]
            #[test]
            fn [<$test_fn _ $count_fn _ $final_epoch>]() {
                let (world, changes_list) = $test_fn($final_epoch);
                $count_fn(world, changes_list);

            }
         }
    };
    ($test_fn:ident, $count_fn:ident, [$($final_epoch:ident) , *]) => {
        $( parametrized_tests!($test_fn, $count_fn, $final_epoch); ) *
    };
    ($test_fn:ident, [$($count_fn:ident) , *], $final_epochs:tt) => {
        $( parametrized_tests!($test_fn, $count_fn, $final_epochs); ) *
    };
    ([$($test_fn:ident) , *], $count_fns:tt, $final_epochs:tt) => {
        $( parametrized_tests!($test_fn, $count_fns, $final_epochs); ) *
    };
}

parametrized_tests!(
    [
        test_spawn_one,
        test_spawn,
        test_spawn_batch,
        test_modify_for_each,
        test_modify_query,
        test_modify_query_one,
        test_modify_for_each_alt,
        test_modify_by_query_one
    ],
    [
        count_for_each_tracked,
        count_query_tracked_fold,
        count_query_tracked_for_loop
    ],
    [
        HALF_OF_MAX_EPOCH
    ]
);
