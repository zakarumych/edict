use core::{
    any::TypeId,
    sync::atomic::{AtomicUsize, Ordering},
};
use edict::{
    Access,
    archetype::Archetype,
    component::{Component, ComponentInfo},
    epoch::EpochId,
    query::{AsQuery, Entities, IntoQuery, Modified, Query, UnitFetch, With, WriteAlias},
    relation::{Related, Relation},
    world::World,
};

static EARLY: AtomicUsize = AtomicUsize::new(0);
static LATE: AtomicUsize = AtomicUsize::new(0);
static FETCH: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct Probe<const EARLY: bool, const LATE: bool>;
impl<const E: bool, const L: bool> AsQuery for Probe<E, L> {
    type Query = Self;
}
impl<const E: bool, const L: bool> IntoQuery for Probe<E, L> {
    fn into_query(self) -> Self {
        self
    }
}
unsafe impl<const E: bool, const L: bool> Query for Probe<E, L> {
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;
    const MUTABLE: bool = false;
    fn cache_visit_archetype(_: &Archetype) -> Option<bool> {
        Some(E)
    }
    fn component_access(&self, _: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }
    fn visit_archetype(&self, _: &Archetype) -> bool {
        EARLY.fetch_add(1, Ordering::Relaxed);
        E
    }
    unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(TypeId, Access)) {}
    unsafe fn visit_archetype_late(&self, _: &Archetype) -> bool {
        LATE.fetch_add(1, Ordering::Relaxed);
        assert!(E);
        L
    }
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        FETCH.fetch_add(1, Ordering::Relaxed);
        assert!(E && L);
        UnitFetch::new()
    }
}

#[test]
fn default_optional_fetch_short_circuits_checks() {
    let mut world = World::new();
    world.spawn(());
    let archetype = &world.archetypes()[0];
    for (early, late, fetch, result) in [(1, 0, 0, false), (1, 1, 0, false), (1, 1, 1, true)] {
        EARLY.store(0, Ordering::Relaxed);
        LATE.store(0, Ordering::Relaxed);
        FETCH.store(0, Ordering::Relaxed);
        // These probes require no component access and borrow no data.
        let actual = unsafe {
            if late == 0 {
                Probe::<false, true>
                    .fetch_optional(0, archetype, world.epoch())
                    .is_some()
            } else if fetch == 0 {
                Probe::<true, false>
                    .fetch_optional(0, archetype, world.epoch())
                    .is_some()
            } else {
                Probe::<true, true>
                    .fetch_optional(0, archetype, world.epoch())
                    .is_some()
            }
        };
        assert_eq!(actual, result);
        assert_eq!(EARLY.load(Ordering::Relaxed), early);
        assert_eq!(LATE.load(Ordering::Relaxed), late);
        assert_eq!(FETCH.load(Ordering::Relaxed), fetch);
    }
}

#[derive(Clone, Copy, Relation)]
struct Directed;
#[derive(Clone, Copy, Relation)]
#[edict(symmetric)]
struct Symmetric;

fn optional_related<R: Relation>(world: &mut World, relation: R, symmetric: bool) {
    let origin = world.spawn(()).id();
    let target = world.spawn(()).id();
    let absent = world.spawn(()).id();
    world.insert_relation(origin, relation, target).unwrap();
    type Q<R> = (Entities, Option<Related<With<R>>>);
    for cached in [false, true] {
        if cached {
            world.maintain_query_cache::<<Q<R> as AsQuery>::Query>();
            world.maintain_query_cache::<<(Q<R>,) as AsQuery>::Query>();
            world.maintain_query_cache::<<Option<Related<With<R>>> as AsQuery>::Query>();
        }
        let mut rows: Vec<_> = world
            .view::<Q<R>>()
            .iter()
            .map(|(id, related)| {
                (
                    id.id(),
                    related.map(|iter| iter.map(|id| id.id()).collect::<Vec<_>>()),
                )
            })
            .collect();
        rows.sort_by_key(|row| row.0);
        let mut expected = vec![
            (origin, symmetric.then_some(vec![target])),
            (target, Some(vec![origin])),
            (absent, None),
        ];
        expected.sort_by_key(|row| row.0);
        assert_eq!(rows, expected);
        {
            let view = world.view::<Q<R>>();
            for entity in world.view::<Entities>().into_iter() {
                let expected = &expected
                    .iter()
                    .find(|(id, _)| *id == entity.id())
                    .unwrap()
                    .1;
                let (_, related) = view.get(entity).unwrap();
                let actual = related.map(|iter| iter.map(|id| id.id()).collect::<Vec<_>>());
                assert_eq!(&actual, expected);
            }
        }
        for (id, expected) in expected {
            let actual = world
                .get::<Option<Related<With<R>>>>(id)
                .unwrap()
                .map(|iter| iter.map(|id| id.id()).collect::<Vec<_>>());
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn optional_related_directed_and_symmetric_cached_and_uncached() {
    optional_related(&mut World::new(), Directed, false);
    optional_related(&mut World::new(), Symmetric, true);
}

#[derive(Component)]
struct Value(i32);

#[test]
fn optional_mutable_fetch_preserves_epochs_and_borrow_conflicts() {
    for cached in [false, true] {
        let mut world = World::new();
        let present = world.spawn((Value(1),)).id();
        let absent = world.spawn(()).id();
        if cached {
            world.maintain_query_cache::<<(Option<&mut Value>,) as AsQuery>::Query>();
        }
        let before = world.epoch();
        for value in world.view::<Option<&mut Value>>().into_iter().flatten() {
            value.0 += 1;
        }
        assert_eq!(world.get::<&Value>(present).unwrap().0, 2);
        assert!(world.get::<Option<&Value>>(absent).unwrap().is_none());
        assert_eq!(
            world
                .view_with(Modified::<With<Value>>::new(before))
                .iter()
                .count(),
            1
        );
        let before = world.epoch();
        world.get::<Option<&mut Value>>(present).unwrap().unwrap().0 += 1;
        assert_eq!(
            world
                .view_with(Modified::<With<Value>>::new(before))
                .iter()
                .count(),
            1
        );

        let held = world.view::<&Value>();
        assert_eq!(held.iter().count(), 1);
        let conflict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.view::<Option<&mut Value>>().into_iter().count();
        }));
        assert!(conflict.is_err());
        drop(held);
        assert_eq!(
            world
                .view::<Option<&mut Value>>()
                .into_iter()
                .flatten()
                .count(),
            1
        );
    }
}
