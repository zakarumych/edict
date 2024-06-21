use alloc::{vec, vec::Vec};

use crate::{
    component::Component,
    query::{Entities, ImmutableQuery, Modified, Not, With, Without},
    relation::{ChildOf, Relation},
    system::{IntoSystem, System},
    view::View,
    world::World,
};

#[cfg(feature = "flow")]
use crate::flow::{self, flow_fn, Flows};

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
    assert!(e.has_component::<U32>());
    assert!(e.has_component::<Str>());
    assert_eq!(e.get::<(&U32, &Str)>(), Some((&U32(42), &Str("qwe"))));
}

/// Tests that entity does not have a component that wasn't in spawn bundle
/// but has it after component is inserted
#[test]
fn world_insert() {
    let mut world = World::new();

    let mut e = world.spawn((U32(42),));
    assert!(e.has_component::<U32>());
    assert!(!e.has_component::<Str>());
    assert_eq!(e.get::<(&U32, &Str)>(), None);

    e.with(|| Str("qwe"));
    assert!(e.has_component::<Str>());
    assert_eq!(e.get::<(&U32, &Str)>(), Some((&U32(42), &Str("qwe"))));
}

/// Tests that entity does not have a component that was removed.
#[test]
fn world_remove() {
    let mut world = World::new();

    let mut e = world.spawn((U32(42), Str("qwe")));
    assert_eq!(e.get::<(&U32, &Str)>(), Some((&U32(42), &Str("qwe"))));
    assert!(e.has_component::<U32>());
    assert!(e.has_component::<Str>());

    assert_eq!(e.remove::<Str>(), Some(Str("qwe")));
    assert!(!e.has_component::<Str>());
    assert_eq!(e.get::<(&U32, &Str)>(), None);
}

/// Insertion test. Bundle version
#[test]
fn world_insert_bundle() {
    let mut world = World::new();

    let mut e = world.spawn((U32(42),));
    assert!(e.has_component::<U32>());
    assert!(!e.has_component::<Str>());
    assert_eq!(e.get::<(&U32, &Str)>(), None);

    e.with_bundle((Str("qwe"), Bool(true)));
    assert!(e.has_component::<Str>());
    assert!(e.has_component::<Bool>());
    assert_eq!(
        e.get::<(&U32, &Str, &Bool)>(),
        Some((&U32(42), &Str("qwe"), &Bool(true)))
    );
}

/// Removing test. Bundle version.
#[test]
fn world_remove_bundle() {
    let mut world = World::new();

    let e = world.spawn((U32(42), Str("qwe")));
    assert!(e.has_component::<U32>());
    assert!(e.has_component::<Str>());
    assert_eq!(e.get::<(&U32, &Str)>(), Some((&U32(42), &Str("qwe"))));

    // When removing a bundle, any missing component is simply ignored.
    let e = e.drop_bundle::<(Str, Bool)>().unwrap();
    assert!(!e.has_component::<Str>());
    assert_eq!(e.get::<(&U32, &Str)>(), None);
}

#[test]
fn version_test() {
    let mut world = World::new();

    let mut epoch = world.epoch();

    let e = world.spawn((U32(42), Str("qwe"))).id();

    let view = world
        .view_mut::<Entities>()
        .modified::<U32>(epoch)
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(view.len(), 1);
    assert_eq!(view[0].0, e);
    assert_eq!(view[0].1, &U32(42));

    epoch = world.epoch();

    assert_eq!(
        world
            .view_mut::<Entities>()
            .modified::<U32>(epoch)
            .into_iter()
            .count(),
        0
    );

    *world
        .try_view_one::<&mut U32>(e)
        .unwrap()
        .get_mut()
        .unwrap() = U32(42);

    let view = world
        .view_mut::<Entities>()
        .modified::<U32>(epoch)
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(view.len(), 1);
    assert_eq!(view[0].0, e);
    assert_eq!(view[0].1, &U32(42));
}

#[test]
fn version_despawn_test() {
    let mut world = World::new();

    let mut epoch = world.epoch();
    let e1 = world.spawn((U32(42), Str("qwe"))).id();
    let e2 = world.spawn((U32(23), Str("rty"))).id();

    let view = world
        .view_mut::<Entities>()
        .modified::<U32>(epoch)
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(view.len(), 2);
    assert_eq!(view[0].0, e1);
    assert_eq!(view[0].1, &U32(42));
    assert_eq!(view[1].0, e2);
    assert_eq!(view[1].1, &U32(23));

    epoch = world.epoch();

    assert_eq!(
        world
            .view_mut::<Entities>()
            .modified::<U32>(epoch)
            .into_iter()
            .collect::<Vec<_>>(),
        vec![]
    );

    *world
        .try_view_one::<&mut U32>(e2)
        .unwrap()
        .get_mut()
        .unwrap() = U32(50);

    world.despawn(e1).unwrap();

    let view = world
        .view_mut::<Entities>()
        .modified::<U32>(epoch)
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(view.len(), 1);
    assert_eq!(view[0].0, e2);
    assert_eq!(view[0].1, &U32(50));
}

#[test]
fn version_insert_test() {
    let mut world = World::new();

    let mut epoch = world.epoch();
    let e1 = world.spawn((U32(42), Str("qwe"))).id();
    let e2 = world.spawn((U32(23), Str("rty"))).id();

    let view = world
        .view_mut::<Entities>()
        .modified::<U32>(epoch)
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(view[0].0, e1);
    assert_eq!(*view[0].1, U32(42));
    assert_eq!(view[1].0, e2);
    assert_eq!(*view[1].1, U32(23));

    epoch = world.epoch();

    assert_eq!(
        world
            .view_mut::<Entities>()
            .modified::<&U32>(epoch)
            .into_iter()
            .count(),
        0
    );

    *world.get::<&mut U32>(e1).unwrap() = U32(50);
    *world.get::<&mut U32>(e2).unwrap() = U32(100);

    assert_eq!(world.insert(e1, Bool(true)), Ok(()));

    let view = world
        .view_mut::<Entities>()
        .modified::<U32>(epoch)
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(view[0].0, e2);
    assert_eq!(*view[0].1, U32(100));
    assert_eq!(view[1].0, e1);
    assert_eq!(*view[1].1, U32(50));
}

#[test]
fn test_relation() {
    let mut world = World::new();

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct A;

    impl Relation for A {
        const EXCLUSIVE: bool = false;
        const SYMMETRIC: bool = false;
    }

    let a = world.spawn(()).id();
    let b = world.spawn(()).id();

    world.add_relation(a, A, a).unwrap();

    let mut a_s = world
        .view_mut::<Entities>()
        .relates::<A>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(a_s.len(), 1);
    assert_eq!(a_s[0].0, a);
    assert_eq!(a_s[0].1.len(), 1);
    let first = a_s[0].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, a);

    assert_eq!(a_s[0].1.by_ref().count(), 0);

    world.add_relation(a, A, b).unwrap();

    let mut a_s = world
        .view_mut::<Entities>()
        .relates::<A>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(a_s.len(), 1);
    assert_eq!(a_s[0].0, a);
    assert_eq!(a_s[0].1.len(), 2);
    let first = a_s[0].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, a);

    let second = a_s[0].1.next().unwrap();
    assert_eq!(second.0, &A);
    assert_eq!(second.1, b);

    assert_eq!(a_s[0].1.by_ref().count(), 0);

    world.despawn(a).unwrap();

    assert_eq!(
        world
            .view_mut::<Entities>()
            .relates::<A>()
            .into_iter()
            .count(),
        0
    );
}

#[test]
fn test_exclusive_relation() {
    let mut world = World::new();

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct A;

    impl Relation for A {
        const EXCLUSIVE: bool = true;
        const SYMMETRIC: bool = false;
    }

    let a = world.spawn(()).id();
    let b = world.spawn(()).id();

    world.add_relation(a, A, a).unwrap();

    let mut a_s = world
        .view_mut::<Entities>()
        .relates::<A>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(a_s.len(), 1);
    assert_eq!(a_s[0].0, a);
    assert_eq!(a_s[0].1.len(), 1);
    let first = a_s[0].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, a);

    assert_eq!(a_s[0].1.by_ref().count(), 0);

    world.add_relation(a, A, b).unwrap();

    let mut a_s = world
        .view_mut::<Entities>()
        .relates::<A>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(a_s.len(), 1);
    assert_eq!(a_s[0].0, a);
    assert_eq!(a_s[0].1.len(), 1);
    let first = a_s[0].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, b);

    assert_eq!(a_s[0].1.by_ref().count(), 0);

    world.despawn(a).unwrap();

    assert_eq!(
        world
            .view_mut::<Entities>()
            .relates::<A>()
            .into_iter()
            .count(),
        0
    );
}

#[test]
fn test_symmetric_relation() {
    let mut world = World::new();

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct A;

    impl Relation for A {
        const EXCLUSIVE: bool = false;
        const SYMMETRIC: bool = true;
    }

    let a = world.spawn(()).id();
    let b = world.spawn(()).id();

    world.add_relation(a, A, a).unwrap();

    let mut a_s = world
        .view_mut::<Entities>()
        .relates::<A>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(a_s.len(), 1);
    assert_eq!(a_s[0].0, a);
    assert_eq!(a_s[0].1.len(), 1);
    let first = a_s[0].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, a);

    assert_eq!(a_s[0].1.by_ref().count(), 0);

    world.add_relation(a, A, b).unwrap();

    let mut a_s = world
        .view_mut::<Entities>()
        .relates::<A>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(a_s.len(), 2);
    assert_eq!(a_s[0].0, a);
    assert_eq!(a_s[0].1.len(), 2);
    let first = a_s[0].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, a);

    let second = a_s[0].1.next().unwrap();
    assert_eq!(second.0, &A);
    assert_eq!(second.1, b);

    assert_eq!(a_s[0].1.by_ref().count(), 0);

    assert_eq!(a_s[1].0, b);
    assert_eq!(a_s[1].1.len(), 1);
    let first = a_s[1].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, a);

    assert_eq!(a_s[1].1.by_ref().count(), 0);

    world.despawn(a).unwrap();

    assert_eq!(
        world
            .view_mut::<Entities>()
            .relates::<A>()
            .into_iter()
            .count(),
        0
    );
}

#[test]
fn test_symmetric_exclusive_relation() {
    let mut world = World::new();

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct A;

    impl Relation for A {
        const EXCLUSIVE: bool = true;
        const SYMMETRIC: bool = true;
    }

    let a = world.spawn(()).id();
    let b = world.spawn(()).id();

    world.add_relation(a, A, a).unwrap();

    let mut a_s = world
        .view_mut::<Entities>()
        .relates::<A>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(a_s.len(), 1);
    assert_eq!(a_s[0].0, a);
    assert_eq!(a_s[0].1.len(), 1);
    let first = a_s[0].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, a);

    assert_eq!(a_s[0].1.by_ref().count(), 0);

    world.add_relation(a, A, b).unwrap();

    let mut a_s = world
        .view_mut::<Entities>()
        .relates::<A>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(a_s.len(), 2);
    assert_eq!(a_s[0].0, a);
    assert_eq!(a_s[0].1.len(), 1);
    let first = a_s[0].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, b);

    assert_eq!(a_s[0].1.by_ref().count(), 0);

    assert_eq!(a_s[1].0, b);
    assert_eq!(a_s[1].1.len(), 1);
    let first = a_s[1].1.next().unwrap();
    assert_eq!(first.0, &A);
    assert_eq!(first.1, a);

    assert_eq!(a_s[1].1.by_ref().count(), 0);

    world.despawn(a).unwrap();

    assert_eq!(
        world
            .view_mut::<Entities>()
            .relates::<A>()
            .into_iter()
            .count(),
        0
    );
}

#[test]
fn test_filters() {
    use crate::query::AsQuery;

    fn is_filter<F: AsQuery>()
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

    let e = world.spawn((A {},)).id();
    let query_filter = Not(With::<B>);
    assert_eq!(
        world
            .try_view_one_with(e, query_filter)
            .unwrap()
            .get()
            .unwrap(),
        ()
    );

    world
        .spawn_batch(vec![(A {}, B {}), (A {}, B {})])
        .spawn_all();
    world.spawn_batch(vec![(A {},), (A {},)]).spawn_all();

    assert_eq!(5, world.view::<(Entities,)>().with::<A>().iter().count());
    assert_eq!(2, world.view::<(Entities,)>().with::<B>().iter().count());
    assert_eq!(3, world.view::<(Entities,)>().without::<B>().iter().count());
    assert_eq!(0, world.view::<(Entities,)>().without::<A>().iter().count());
}

#[test]
fn add_relation() {
    let mut world = World::new();

    let target = world.allocate().id();
    let origin = world.allocate().id();

    #[derive(Component)]
    struct Foo;

    world.insert(origin, Foo).unwrap();
    world.add_relation(origin, ChildOf, target).unwrap();
}

#[cfg(feature = "flow")]
#[test]
fn test_flow() {
    let mut world = World::new();

    assert_eq!(world.view::<&U32>().iter().count(), 0);

    world.spawn_flow(flow_fn!(|world: &mut flow::World| {
        world.spawn((U32(42),));
    }));

    assert_eq!(world.view::<&U32>().iter().count(), 0);
    Flows::default().execute(&mut world);
    assert_eq!(world.view::<&U32>().iter().count(), 1);
}

#[cfg(feature = "flow")]
#[test]
fn test_entity_flow() {
    let mut world = World::new();

    let e = world.spawn(()).id();

    assert_eq!(world.view::<&U32>().iter().count(), 0);

    world.spawn_flow_for(
        e,
        flow_fn!(|mut e: flow::Entity| {
            e.insert(U32(42));
        }),
    );

    assert_eq!(world.view::<&U32>().iter().count(), 0);
    Flows::default().execute(&mut world);
    assert_eq!(world.view::<&U32>().iter().count(), 1);
}

#[test]
fn test_aliasing_borrows() {
    let mut world = World::new();

    world.spawn_one(U32(42));

    let system = |_: View<Modified<&U32>>, _: View<&U32>| {};

    system.into_system().run_alone(&mut world);
}
