use crate::{
    component::Component,
    query::{Entities, ImmutableQuery, Not, With, Without},
    relation::{ChildOf, Relation, RelationOrigin, RelationTarget},
    world::{QueryOneError, World},
};

use alloc::{vec, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Str(&'static str);
impl Component for Str {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct U32(u32);
impl Component for U32 {}

#[derive(Debug, PartialEq, Eq)]
struct Bool(bool);
impl Component for Bool {}

/// Tests that entity spawned into world has all components from bundle.
#[test]
fn world_spawn() {
    let mut world = World::new();

    let e = world.spawn((U32(42), Str("qwe")));
    assert_eq!(world.has_component::<U32>(e), Ok(true));
    assert_eq!(world.has_component::<Str>(e), Ok(true));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str)>(e),
        Ok((&U32(42), &Str("qwe")))
    );
}

/// Tests that entity does not have a component that wasn't in spawn bundle
/// but has it after component is inserted
#[test]
fn world_insert() {
    let mut world = World::new();

    let e = world.spawn((U32(42),));
    assert_eq!(world.has_component::<U32>(e), Ok(true));
    assert_eq!(world.has_component::<Str>(e), Ok(false));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str)>(e),
        Err(QueryOneError::NotSatisfied)
    );

    assert_eq!(world.insert(e, Str("qwe")), Ok(()));
    assert_eq!(world.has_component::<Str>(e), Ok(true));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str)>(e),
        Ok((&U32(42), &Str("qwe")))
    );
}

/// Tests that entity does not have a component that was removed.
#[test]
fn world_remove() {
    let mut world = World::new();

    let e = world.spawn((U32(42), Str("qwe")));
    assert_eq!(world.has_component::<U32>(e), Ok(true));
    assert_eq!(world.has_component::<Str>(e), Ok(true));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str)>(e),
        Ok((&U32(42), &Str("qwe")))
    );

    assert_eq!(world.remove::<Str>(e), Ok(Str("qwe")));
    assert_eq!(world.has_component::<Str>(e), Ok(false));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str)>(e),
        Err(QueryOneError::NotSatisfied)
    );
}

/// Insertion test. Bundle version
#[test]
fn world_insert_bundle() {
    let mut world = World::new();

    let e = world.spawn((U32(42),));
    assert_eq!(world.has_component::<U32>(e), Ok(true));
    assert_eq!(world.has_component::<Str>(e), Ok(false));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str)>(e),
        Err(QueryOneError::NotSatisfied)
    );

    assert_eq!(world.insert_bundle(e, (Str("qwe"), Bool(true))), Ok(()));
    assert_eq!(world.has_component::<Str>(e), Ok(true));
    assert_eq!(world.has_component::<Bool>(e), Ok(true));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str, &Bool)>(e),
        Ok((&U32(42), &Str("qwe"), &Bool(true)))
    );
}

/// Removing test. Bundle version.
#[test]
fn world_remove_bundle() {
    let mut world = World::new();

    let e = world.spawn((U32(42), Str("qwe")));
    assert_eq!(world.has_component::<U32>(e), Ok(true));
    assert_eq!(world.has_component::<Str>(e), Ok(true));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str)>(e),
        Ok((&U32(42), &Str("qwe")))
    );

    // When removing a bundle, any missing component is simply ignored.
    assert_eq!(world.drop_bundle::<(Str, Bool)>(e), Ok(()));
    assert_eq!(world.has_component::<Str>(e), Ok(false));
    assert_eq!(
        world.query_one_mut::<(&U32, &Str)>(e),
        Err(QueryOneError::NotSatisfied)
    );
}

#[test]
fn version_test() {
    let mut world = World::new();

    let mut epoch = world.epoch();

    let e = world.spawn((U32(42), Str("qwe")));

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![(e, &U32(42))]
    );

    epoch = world.epoch();

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![]
    );

    *world.query_one_mut::<&mut U32>(e).unwrap() = U32(42);

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![(e, &U32(42))]
    );
}

#[test]
fn version_despawn_test() {
    let mut world = World::new();

    let mut epoch = world.epoch();
    let e1 = world.spawn((U32(42), Str("qwe")));
    let e2 = world.spawn((U32(23), Str("rty")));

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![(e1, &U32(42)), (e2, &U32(23))]
    );

    epoch = world.epoch();

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![]
    );

    *world.query_one_mut::<&mut U32>(e2).unwrap() = U32(50);
    assert_eq!(world.despawn(e1), Ok(()));

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![(e2, &U32(50))]
    );
}

#[test]
fn version_insert_test() {
    let mut world = World::new();

    let mut epoch = world.epoch();
    let e1 = world.spawn((U32(42), Str("qwe")));
    let e2 = world.spawn((U32(23), Str("rty")));

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![(e1, &U32(42)), (e2, &U32(23))]
    );

    epoch = world.epoch();

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![]
    );

    *world.query_one_mut::<&mut U32>(e1).unwrap() = U32(50);
    *world.query_one_mut::<&mut U32>(e2).unwrap() = U32(100);

    assert_eq!(world.insert(e1, Bool(true)), Ok(()));

    assert_eq!(
        world
            .query::<Entities>()
            .modified::<&U32>(epoch)
            .iter()
            .collect::<Vec<_>>(),
        vec![(e2, &U32(100)), (e1, &U32(50))]
    );
}

#[test]
fn test_relation() {
    let mut world = World::new();

    #[derive(Clone, Copy)]
    struct A;

    impl Relation for A {
        const EXCLUSIVE: bool = false;
        const SYMMETRIC: bool = false;
    }

    let a = world.spawn(());
    let b = world.spawn(());

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }

    world.add_relation(a, A, a).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 1);
        assert_eq!(origins[0].targets()[0], a);
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], a);
    }

    world.add_relation(a, A, b).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 2);
        assert_eq!(origins[0].targets()[0], a);
        assert_eq!(origins[0].targets()[1], b);
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert!(a == e || b == e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], a);
    }

    world.despawn(a).unwrap();

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }
}

#[test]
fn test_exclusive_relation() {
    let mut world = World::new();

    #[derive(Clone, Copy)]
    struct A;

    impl Relation for A {
        const EXCLUSIVE: bool = true;
        const SYMMETRIC: bool = false;
    }

    let a = world.spawn(());
    let b = world.spawn(());

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }

    world.add_relation(a, A, a).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 1);
        assert_eq!(origins[0].targets()[0], a);
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], a);
    }

    world.add_relation(a, A, b).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 1);
        assert_eq!(origins[0].targets()[0], b);
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert_eq!(b, e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], a);
    }

    world.add_relation(a, A, a).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 1);
        assert_eq!(origins[0].targets()[0], a);
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], a);
    }

    world.despawn(a).unwrap();

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }
}

#[test]
fn test_symmetric_relation() {
    let mut world = World::new();

    #[derive(Clone, Copy)]
    struct A;

    impl Relation for A {
        const EXCLUSIVE: bool = false;
        const SYMMETRIC: bool = true;
    }

    let a = world.spawn(());
    let b = world.spawn(());

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }

    world.add_relation(a, A, a).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 1);
        assert_eq!(origins[0].targets()[0], a);
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], a);
    }

    world.despawn(a).unwrap();

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }

    let a = world.spawn(());

    world.add_relation(a, A, b).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert!(a == e || b == e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 1);
        assert_eq!(origins[0].targets()[0], if a == e { b } else { a });
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert!(a == e || b == e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], if a == e { b } else { a });
    }

    world.despawn(a).unwrap();

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }
}

#[test]
fn test_symmetric_exclusive_relation() {
    let mut world = World::new();

    #[derive(Clone, Copy)]
    struct A;

    impl Relation for A {
        const EXCLUSIVE: bool = true;
        const SYMMETRIC: bool = true;
    }

    let a = world.spawn(());
    let b = world.spawn(());

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }

    world.add_relation(a, A, a).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 1);
        assert_eq!(origins[0].targets()[0], a);
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert_eq!(a, e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], a);
    }

    world.add_relation(a, A, b).unwrap();

    for (e, origins) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        assert!(a == e || b == e);
        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0].targets().len(), 1);
        assert_eq!(origins[0].targets()[0], if a == e { b } else { a });
    }

    for (e, targets) in world
        .query::<Entities>()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        assert!(a == e || b == e);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].origins().len(), 1);
        assert_eq!(targets[0].origins()[0], if a == e { b } else { a });
    }

    world.despawn(a).unwrap();

    for _origins in world
        .new_query()
        .borrow_all::<&(dyn RelationOrigin + Sync)>()
        .iter()
    {
        panic!()
    }

    for _targets in world
        .new_query()
        .borrow_all::<&(dyn RelationTarget + Sync)>()
        .iter()
    {
        panic!()
    }
}

#[test]
fn test_filters() {
    use crate::query::IntoQuery;

    fn is_filter<F: IntoQuery>()
    where
        F::Query: ImmutableQuery,
    {
    }
    is_filter::<()>();
    is_filter::<((), ())>();

    struct A;
    is_filter::<With<A>>();
    is_filter::<Without<A>>();
}

#[test]
fn with_without() {
    let mut world = World::new();

    #[derive(Debug, Component, PartialEq)]
    struct A;
    #[derive(Component, PartialEq)]
    struct B;

    let e = world.spawn((A {},));
    let query_filter = Not(With::<B>::query());
    assert_eq!(
        world
            .query_one_with(e, query_filter)
            .unwrap()
            .get()
            .unwrap(),
        ()
    );

    world
        .spawn_batch(vec![(A {}, B {}), (A {}, B {})])
        .spawn_all();
    world.spawn_batch(vec![(A {},), (A {},)]).spawn_all();

    assert_eq!(5, world.query::<(Entities,)>().with::<A>().iter().count());
    assert_eq!(2, world.query::<(Entities,)>().with::<B>().iter().count());
    assert_eq!(
        3,
        world.query::<(Entities,)>().without::<B>().iter().count()
    );
    assert_eq!(
        0,
        world.query::<(Entities,)>().without::<A>().iter().count()
    );
}

#[test]
fn add_relation() {
    let mut world = World::new();

    let target = world.allocate();
    let origin = world.allocate();

    #[derive(Component)]
    struct Foo;

    world.insert(origin, Foo).unwrap();
    world.add_relation(origin, ChildOf, target).unwrap();
}
