use core::any::TypeId;
use edict::{
    Access,
    archetype::Archetype,
    component::{Component, ComponentInfo},
    epoch::EpochId,
    query::{
        Alt, AsQuery, Cpy, DefaultQuery, Entities, EpochOf, Fetch, IntoQuery, Modified, Query,
        Read, SendQuery, With, WithEpoch, WriteAlias,
    },
    relation::{Related, Relation},
    world::World,
};

#[derive(Clone, Copy, Component)]
struct A(u32);
#[derive(Clone, Copy, Component)]
struct B(u32);
#[derive(Clone, Copy, Component)]
struct C(u32);
#[derive(Component)]
struct Tag<const N: usize>;

#[test]
fn nested_optional_tuples_match_present_and_absent_components() {
    type Q = (Cpy<A>, Option<&'static B>, Option<Option<Cpy<C>>>);
    for cached in [false, true] {
        let mut world = World::new();
        let both = world.spawn((A(1), B(2), C(3))).id();
        let a = world.spawn((A(4),)).id();
        let absent = world.spawn((C(5),)).id();
        if cached {
            world.maintain_query_cache::<<(Option<Q>,) as AsQuery>::Query>();
            world.maintain_query_cache::<<Option<Q> as AsQuery>::Query>();
        }
        let view = world.view::<Option<Q>>();
        let (x, y, z) = view.try_get(both).unwrap().unwrap();
        assert_eq!((x.0, y.unwrap().0, z.unwrap().unwrap().0), (1, 2, 3));
        let (x, y, z) = view.try_get(a).unwrap().unwrap();
        assert_eq!(x.0, 4);
        assert!(y.is_none());
        assert!(matches!(z, Some(None)));
        assert!(view.try_get(absent).unwrap().is_none());
        assert_eq!(view.iter().count(), 3);
    }
}

#[test]
fn prepared_metadata_refreshes_payload_and_epoch_buffers_after_growth() {
    type Q = (Entities, Cpy<A>, Read<B>, EpochOf<A>);
    let mut world = World::new();
    let first = world.spawn((A(7), B(11))).id();
    world.maintain_query_cache::<Q>();
    world.maintain_query_cache::<(Q,)>();
    world.maintain_query_cache::<<(Alt<A>,) as AsQuery>::Query>();
    world.maintain_query_cache::<<(WithEpoch<&'static mut A>,) as AsQuery>::Query>();
    for index in 0..4096 {
        world.spawn((A(index), B(index + 1)));
    }
    // New schemas exercise fallback rows while the original schema's buffers move.
    world.spawn((A(13), B(17), Tag::<0>));
    world.spawn((A(19), B(23), Tag::<1>));
    assert_eq!(world.view::<Q>().iter().count(), 4099);
    assert_eq!(world.view::<Q>().try_get(first).unwrap().1.0, 7);
    assert_eq!(world.view::<Q>().try_get(first).unwrap().2.0, 11);
    for step in 0..3 {
        let cutoff = world.epoch();
        {
            let mut view = world.view::<WithEpoch<&mut A>>();
            let (a, previous) = view.try_get_mut(first).unwrap();
            assert!(!previous.after(cutoff));
            a.0 = 100 + step;
        }
        assert_eq!(
            world
                .view_with(Modified::<With<A>>::new(cutoff))
                .iter()
                .count(),
            1
        );
        let cutoff = world.epoch();
        {
            let mut view = world.view::<Alt<A>>();
            let a = view.try_get_mut(first).unwrap();
            assert_eq!(a.0, 100 + step);
        }
        assert_eq!(
            world
                .view_with(Modified::<With<A>>::new(cutoff))
                .iter()
                .count(),
            0
        );
        {
            let mut view = world.view::<Alt<A>>();
            view.try_get_mut(first).unwrap().0 += 1;
        }
        assert_eq!(
            world
                .view_with(Modified::<With<A>>::new(cutoff))
                .iter()
                .count(),
            1
        );
    }
}

#[derive(Clone, Copy, Relation)]
struct Directed;
#[derive(Clone, Copy, Relation)]
#[edict(symmetric)]
struct Symmetric;

fn related_growth<R: Relation>(relation: R) {
    let mut world = World::new();
    let target = world.spawn((A(0),)).id();
    let origin = world.spawn((A(1),)).id();
    let absent = world.spawn((B(0),)).id();
    world.insert_relation(origin, relation, target).unwrap();
    type Q<R> = (Entities, Option<Related<With<R>>>);
    world.maintain_query_cache::<<Q<R> as AsQuery>::Query>();
    world.maintain_query_cache::<<(Q<R>,) as AsQuery>::Query>();
    for index in 0..1024 {
        let a = world.spawn((A(index),)).id();
        let b = world.spawn((A(index),)).id();
        world.insert_relation(a, relation, b).unwrap();
    }
    let view = world.view::<Q<R>>();
    assert_eq!(
        view.try_get(target)
            .unwrap()
            .1
            .unwrap()
            .map(|e| e.id())
            .collect::<Vec<_>>(),
        vec![origin]
    );
    assert!(view.try_get(absent).unwrap().1.is_none());
    assert_eq!(view.iter().count(), 2051);
    drop(view);
    // Ordinary fetches share the same fresh component-data preparation.
    assert_eq!(
        world
            .get::<Related<With<R>>>(target)
            .unwrap()
            .map(|e| e.id())
            .collect::<Vec<_>>(),
        vec![origin]
    );
}

#[test]
fn cached_related_fetches_survive_directed_and_symmetric_storage_growth() {
    related_growth(Directed);
    related_growth(Symmetric);
}

#[derive(Clone, Copy)]
struct Strict;
impl AsQuery for Strict {
    type Query = Self;
}
impl IntoQuery for Strict {
    fn into_query(self) -> Self {
        self
    }
}
impl DefaultQuery for Strict {
    fn default_query() -> Self {
        Self
    }
}
struct StrictFetch {
    epoch: EpochId,
    stage: u8,
}
unsafe impl<'a> Fetch<'a> for StrictFetch {
    type Item = EpochId;
    fn dangling() -> Self {
        Self {
            epoch: EpochId::start(),
            stage: 99,
        }
    }
    unsafe fn visit_chunk(&mut self, _: u32) -> bool {
        assert_eq!(self.stage, 0);
        self.stage = 1;
        true
    }
    unsafe fn touch_chunk(&mut self, _: u32) {
        assert_eq!(self.stage, 1);
        self.stage = 2;
    }
    unsafe fn visit_item(&mut self, _: u32) -> bool {
        assert_eq!(self.stage, 2);
        self.stage = 3;
        true
    }
    unsafe fn get_item(&mut self, _: u32) -> EpochId {
        assert_eq!(self.stage, 3);
        self.stage = 4;
        self.epoch
    }
}
unsafe impl Query for Strict {
    type Item<'a> = EpochId;
    type Fetch<'a> = StrictFetch;
    const MUTABLE: bool = true;
    const FILTERS_ENTITIES: bool = true;
    fn cache_visit_archetype(_: &Archetype) -> Option<bool> {
        Some(true)
    }
    fn component_access(&self, _: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }
    fn visit_archetype(&self, _: &Archetype) -> bool {
        true
    }
    unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(TypeId, Access)) {}
    unsafe fn fetch(&self, _: u32, _: &Archetype, epoch: EpochId) -> StrictFetch {
        StrictFetch { epoch, stage: 0 }
    }
}
unsafe impl SendQuery for Strict {}

#[test]
fn tuple_cache_keeps_custom_fallback_fresh_and_mutable_fetches_strict() {
    type Q = (Strict, Alt<A>);
    let mut world = World::new();
    let id = world.spawn((A(0),)).id();
    assert_eq!(Strict::FETCH_CACHE_SLOTS, 0);
    assert_eq!(Q::FETCH_CACHE_SLOTS, 1);
    world.maintain_query_cache::<Q>();
    world.maintain_query_cache::<(Q,)>();
    let mut previous = world.epoch();
    let mut view = world.view::<Q>();
    for _ in 0..600 {
        let (epoch, mut a) = view.try_get_mut(id).unwrap();
        assert!(epoch.after(previous));
        a.0 += 1;
        previous = epoch;
    }
}

#[test]
fn ordinary_cached_writes_refresh_storage_and_keep_optional_absence() {
    let mut world = World::new();
    let first = world.spawn((A(0),)).id();
    let second = world.spawn((A(0),)).id();
    let absent = world.spawn((B(0),)).id();
    world.maintain_query_cache::<edict::query::Write<A>>();
    world.maintain_query_cache::<<(&'static mut A,) as AsQuery>::Query>();
    world.maintain_query_cache::<<Option<&'static mut A> as AsQuery>::Query>();
    world.maintain_query_cache::<<(Option<&'static mut A>,) as AsQuery>::Query>();
    for index in 0..4096 {
        world.spawn((A(index),));
    }
    let cutoff = world.epoch();
    {
        let mut view = world.view::<&mut A>();
        for step in 0..600 {
            view.try_get_mut(if step % 2 == 0 { first } else { second })
                .unwrap()
                .0 += 1;
        }
    }
    assert_eq!(
        world
            .view_with(Modified::<With<A>>::new(cutoff))
            .iter()
            .count(),
        2
    );
    assert_eq!(world.get::<&A>(first).unwrap().0, 300);
    assert_eq!(world.get::<&A>(second).unwrap().0, 300);
    for _ in 0..3 {
        let before = world.epoch();
        world.get::<&mut A>(first).unwrap().0 += 1;
        assert!(world.get::<EpochOf<A>>(first).unwrap().after(before));
    }
    let cutoff = world.epoch();
    {
        let mut view = world.view::<Option<&mut A>>();
        assert!(view.try_get_mut(absent).unwrap().is_none());
        view.try_get_mut(second).unwrap().unwrap().0 += 1;
    }
    assert_eq!(
        world
            .view_with(Modified::<With<A>>::new(cutoff))
            .iter()
            .count(),
        1
    );
    assert_eq!(world.get::<&A>(first).unwrap().0, 303);
    assert_eq!(world.get::<&A>(second).unwrap().0, 301);
}
