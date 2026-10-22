use edict::{
    action::LocalActionEncoder,
    entity::EntityId,
    query::With,
    relation::{ChildOf, Related, Relates, Relation},
    world::World,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Relation)]
struct Link(i32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Relation)]
#[edict(exclusive)]
struct Choice(i32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Relation)]
#[edict(symmetric)]
struct Friend(i32);
#[derive(Clone, Copy, Debug, Relation)]
#[edict(owned)]
struct Owned;

fn targets<R: Relation>(world: &mut World, origin: EntityId) -> Vec<EntityId> {
    world
        .get::<Relates<With<R>>>(origin)
        .map(|iter| iter.map(|entity| entity.id()).collect())
        .unwrap_or_default()
}
fn origins<R: Relation>(world: &mut World, target: EntityId) -> Vec<EntityId> {
    world
        .get::<Related<With<R>>>(target)
        .map(|iter| iter.map(|entity| entity.id()).collect())
        .unwrap_or_default()
}
fn pair(world: &mut World) -> (EntityId, EntityId) {
    (world.spawn(()).id(), world.spawn(()).id())
}

#[test]
fn multiple_targets_and_reverse_origins_are_tracked() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    let c = world.spawn(()).id();
    world.insert_relation(a, Link(1), b).unwrap();
    world.insert_relation(a, Link(2), c).unwrap();
    world.insert_relation(c, Link(3), b).unwrap();
    let mut outgoing = targets::<Link>(&mut world, a);
    outgoing.sort();
    let mut expected = vec![b, c];
    expected.sort();
    assert_eq!(outgoing, expected);
    let mut incoming = origins::<Link>(&mut world, b);
    incoming.sort();
    let mut expected = vec![a, c];
    expected.sort();
    assert_eq!(incoming, expected);
}

#[test]
fn reinserting_same_pair_replaces_value_without_duplicate() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    world.insert_relation(a, Link(1), b).unwrap();
    world.insert_relation(a, Link(2), b).unwrap();
    assert_eq!(
        world
            .get::<Relates<&Link>>(a)
            .unwrap()
            .map(|(r, id)| (*r, id.id()))
            .collect::<Vec<_>>(),
        vec![(Link(2), b)]
    );
    assert_eq!(origins::<Link>(&mut world, b), vec![a]);
}

#[test]
fn removal_preserves_other_edges_and_is_idempotent() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    let c = world.spawn(()).id();
    world.insert_relation(a, Link(1), b).unwrap();
    world.insert_relation(a, Link(2), c).unwrap();
    assert_eq!(world.remove_relation::<Link>(a, b).unwrap(), Some(Link(1)));
    assert_eq!(world.remove_relation::<Link>(a, b).unwrap(), None);
    assert_eq!(targets::<Link>(&mut world, a), vec![c]);
    assert!(origins::<Link>(&mut world, b).is_empty());
}

#[test]
fn exclusive_replacement_cleans_previous_target() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    let c = world.spawn(()).id();
    world.insert_relation(a, Choice(1), b).unwrap();
    world.insert_relation(a, Choice(2), c).unwrap();
    assert_eq!(targets::<Choice>(&mut world, a), vec![c]);
    assert!(origins::<Choice>(&mut world, b).is_empty());
    assert_eq!(origins::<Choice>(&mut world, c), vec![a]);
}

#[test]
fn symmetric_relation_removal_cleans_both_directions() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    world.insert_relation(a, Friend(4), b).unwrap();
    assert_eq!(targets::<Friend>(&mut world, a), vec![b]);
    assert_eq!(targets::<Friend>(&mut world, b), vec![a]);
    assert_eq!(
        world.remove_relation::<Friend>(b, a).unwrap(),
        Some(Friend(4))
    );
    assert!(targets::<Friend>(&mut world, a).is_empty());
    assert!(targets::<Friend>(&mut world, b).is_empty());
}

#[test]
fn symmetric_self_relation_has_single_edge() {
    let mut world = World::new();
    let a = world.spawn(()).id();
    world.insert_relation(a, Friend(1), a).unwrap();
    world.insert_relation(a, Friend(2), a).unwrap();
    assert_eq!(targets::<Friend>(&mut world, a), vec![a]);
    assert_eq!(
        world.remove_relation::<Friend>(a, a).unwrap(),
        Some(Friend(2))
    );
    assert!(targets::<Friend>(&mut world, a).is_empty());
}

#[test]
fn despawn_target_cleans_surviving_origin() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    let c = world.spawn(()).id();
    world.insert_relation(a, Link(1), b).unwrap();
    world.insert_relation(a, Link(2), c).unwrap();
    world.despawn(b).unwrap();
    assert_eq!(targets::<Link>(&mut world, a), vec![c]);
    assert!(world.is_alive(a));
}

#[test]
fn despawn_origin_cleans_surviving_targets() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    world.insert_relation(a, Link(1), b).unwrap();
    world.despawn(a).unwrap();
    assert!(origins::<Link>(&mut world, b).is_empty());
    assert!(world.is_alive(b));
}

#[test]
fn child_of_cascades_through_multiple_generations() {
    let mut world = World::new();
    let (root, child) = pair(&mut world);
    let grandchild = world.spawn(()).id();
    let sibling = world.spawn(()).id();
    world.insert_relation(child, ChildOf, root).unwrap();
    world.insert_relation(grandchild, ChildOf, child).unwrap();
    world.insert_relation(sibling, ChildOf, root).unwrap();
    world.despawn(root).unwrap();
    for id in [root, child, grandchild, sibling] {
        assert!(!world.is_alive(id));
    }
}

#[test]
fn child_of_reparenting_detaches_old_parent() {
    let mut world = World::new();
    let (child, old) = pair(&mut world);
    let new = world.spawn(()).id();
    world.insert_relation(child, ChildOf, old).unwrap();
    world.insert_relation(child, ChildOf, new).unwrap();
    world.despawn(old).unwrap();
    assert!(world.is_alive(child));
    assert_eq!(targets::<ChildOf>(&mut world, child), vec![new]);
    world.despawn(new).unwrap();
    assert!(!world.is_alive(child));
}

#[test]
fn removing_child_of_allows_child_to_outlive_parent() {
    let mut world = World::new();
    let (child, parent) = pair(&mut world);
    world.insert_relation(child, ChildOf, parent).unwrap();
    assert!(
        world
            .remove_relation::<ChildOf>(child, parent)
            .unwrap()
            .is_some()
    );
    world.despawn(parent).unwrap();
    assert!(world.is_alive(child));
}

#[test]
fn owned_relation_waits_until_last_target_despawns() {
    let mut world = World::new();
    let (origin, a) = pair(&mut world);
    let b = world.spawn(()).id();
    world.insert_relation(origin, Owned, a).unwrap();
    world.insert_relation(origin, Owned, b).unwrap();
    world.despawn(a).unwrap();
    assert!(world.is_alive(origin));
    assert_eq!(targets::<Owned>(&mut world, origin), vec![b]);
    world.despawn(b).unwrap();
    assert!(!world.is_alive(origin));
}

#[derive(Clone, Copy)]
struct Hooks {
    value: i32,
    drop_replaced: bool,
}
impl Relation for Hooks {
    fn on_replace(
        old: &mut Self,
        new: &Self,
        origin: EntityId,
        old_target: EntityId,
        new_target: EntityId,
        mut encoder: LocalActionEncoder,
    ) -> bool {
        let value = old.value;
        let next = new.value;
        encoder.closure(move |world| {
            world
                .with_default_resource::<Vec<(String, EntityId, EntityId, i32)>>()
                .push((
                    format!("replace:{next}:{}", new_target),
                    origin,
                    old_target,
                    value,
                ))
        });
        new.drop_replaced
    }
    fn on_drop(self, origin: EntityId, target: EntityId, mut encoder: LocalActionEncoder) {
        encoder.closure(move |world| {
            world
                .with_default_resource::<Vec<(String, EntityId, EntityId, i32)>>()
                .push(("drop".into(), origin, target, self.value))
        });
    }
}
#[test]
fn relation_drop_hooks_distinguish_remove_replace_and_drop() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    world
        .insert_relation(
            a,
            Hooks {
                value: 1,
                drop_replaced: true,
            },
            b,
        )
        .unwrap();
    assert!(world.remove_relation::<Hooks>(a, b).unwrap().is_some());
    assert!(
        world
            .get_resource::<Vec<(String, EntityId, EntityId, i32)>>()
            .is_none()
    );
    world
        .insert_relation(
            a,
            Hooks {
                value: 2,
                drop_replaced: true,
            },
            b,
        )
        .unwrap();
    world
        .insert_relation(
            a,
            Hooks {
                value: 3,
                drop_replaced: false,
            },
            b,
        )
        .unwrap();
    world
        .insert_relation(
            a,
            Hooks {
                value: 4,
                drop_replaced: true,
            },
            b,
        )
        .unwrap();
    world.drop_relation::<Hooks>(a, b).unwrap();
    let events = world.expect_resource::<Vec<(String, EntityId, EntityId, i32)>>();
    assert_eq!(
        events.iter().map(|e| e.3).collect::<Vec<_>>(),
        vec![2, 3, 3, 4]
    );
    assert!(events[0].0.starts_with("replace:3:"));
    assert!(events[1].0.starts_with("replace:4:"));
    assert_eq!(events[2].0, "drop");
    assert_eq!(events[3].0, "drop");
    assert!(events.iter().all(|e| e.1 == a && e.2 == b));
}

#[test]
fn directed_self_relation_tracks_incoming_and_outgoing_once() {
    let mut world = World::new();
    let entity = world.spawn(()).id();
    world.insert_relation(entity, Link(8), entity).unwrap();
    assert_eq!(targets::<Link>(&mut world, entity), vec![entity]);
    assert_eq!(origins::<Link>(&mut world, entity), vec![entity]);
    assert_eq!(
        world.remove_relation::<Link>(entity, entity).unwrap(),
        Some(Link(8))
    );
    assert!(targets::<Link>(&mut world, entity).is_empty());
    assert!(origins::<Link>(&mut world, entity).is_empty());
}

#[derive(Clone, Copy)]
struct Lifecycle;
impl Relation for Lifecycle {
    fn on_origin_drop(
        origin: EntityId,
        targets: &[(EntityId, Self)],
        mut encoder: LocalActionEncoder,
    ) {
        let targets = targets.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        encoder.closure(move |world| {
            world
                .with_default_resource::<Vec<(bool, EntityId, Vec<EntityId>)>>()
                .push((true, origin, targets))
        });
    }
    fn on_target_drop(
        origins: &[(EntityId, Self)],
        target: EntityId,
        mut encoder: LocalActionEncoder,
    ) {
        let origins = origins.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        encoder.closure(move |world| {
            world
                .with_default_resource::<Vec<(bool, EntityId, Vec<EntityId>)>>()
                .push((false, target, origins))
        });
    }
}

#[test]
fn relation_lifecycle_hooks_report_endpoints_before_cleanup() {
    let mut world = World::new();
    let (a, b) = pair(&mut world);
    let c = world.spawn(()).id();
    world.insert_relation(a, Lifecycle, b).unwrap();
    world.insert_relation(a, Lifecycle, c).unwrap();
    world.despawn(b).unwrap();
    assert_eq!(
        *world.expect_resource::<Vec<(bool, EntityId, Vec<EntityId>)>>(),
        vec![(false, b, vec![a])]
    );
    world.despawn(a).unwrap();
    assert_eq!(
        *world.expect_resource::<Vec<(bool, EntityId, Vec<EntityId>)>>(),
        vec![(false, b, vec![a]), (true, a, vec![c])]
    );
    assert!(origins::<Lifecycle>(&mut world, c).is_empty());
}
