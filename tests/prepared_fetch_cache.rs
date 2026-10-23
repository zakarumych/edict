use edict::{
    Access,
    archetype::Archetype,
    bundle::EntityBuilder,
    component::{Component, ComponentInfo},
    entity::EntityId,
    epoch::EpochId,
    query::{
        AsQuery, Cpy, DefaultQuery, Entities, FetchCacheSlot, ImmutableQuery, IntoQuery, Query,
        SendQuery, UnitFetch, WriteAlias,
    },
    view::{LocalViewRef, ViewRef},
    world::World,
};
use std::{
    any::TypeId,
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct A(i32);
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct B(i32);
#[derive(Component)]
struct Tag<const N: usize>;

type Rows = <((Entities, Cpy<A>, Option<Cpy<B>>),) as AsQuery>::Query;
fn rows(world: &mut World, local: bool) -> BTreeMap<EntityId, (i32, Option<i32>)> {
    if local {
        world
            .local()
            .view::<(Entities, Cpy<A>, Option<Cpy<B>>)>()
            .iter()
            .map(|(e, a, b)| (e.id(), (a.0, b.map(|b| b.0))))
            .collect()
    } else {
        world
            .view::<(Entities, Cpy<A>, Option<Cpy<B>>)>()
            .iter()
            .map(|(e, a, b)| (e.id(), (a.0, b.map(|b| b.0))))
            .collect()
    }
}
fn assert_rows(world: &mut World, expected: &BTreeMap<EntityId, (i32, Option<i32>)>) {
    assert_eq!(&rows(world, false), expected);
    assert_eq!(&rows(world, true), expected);
}

#[test]
fn metadata_survives_world_moves_growth_migration_and_unmaintained_rows() {
    let mut world = World::new();
    let first = world.spawn_one(A(0)).id();
    let mut expected = BTreeMap::from([(first, (0, None))]);
    world.maintain_query_cache::<Rows>();
    world.maintain_query_cache::<(Cpy<A>,)>();
    let mut world = Box::new(world);
    for value in 1..4096 {
        let entity = world.spawn_one(A(value)).id();
        expected.insert(entity, (value, None));
    }
    assert_rows(&mut world, &expected);
    for (&entity, value) in expected.iter_mut().step_by(7) {
        world.insert(entity, B(-value.0)).unwrap();
        value.1 = Some(-value.0);
    }
    for mask in 0..64 {
        let mut builder = EntityBuilder::new();
        builder.add(A(10_000 + mask));
        if mask & 1 != 0 {
            builder.add(Tag::<0>);
        }
        if mask & 2 != 0 {
            builder.add(Tag::<1>);
        }
        if mask & 4 != 0 {
            builder.add(Tag::<2>);
        }
        if mask & 8 != 0 {
            builder.add(Tag::<3>);
        }
        if mask & 16 != 0 {
            builder.add(Tag::<4>);
        }
        if mask & 32 != 0 {
            builder.add(Tag::<5>);
        }
        let entity = world.spawn(builder).id();
        expected.insert(entity, (10_000 + mask, None));
    }
    // Existing rows are prepared; newly created archetypes must use fresh fallback fetches.
    assert_rows(&mut world, &expected);
    world.maintain_query_cache::<Rows>();
    assert_rows(&mut world, &expected);
    let removed: Vec<_> = expected.keys().copied().skip(1).step_by(11).collect();
    for entity in removed {
        world.despawn(entity).unwrap();
        expected.remove(&entity);
    }
    world.as_mut().insert(first, B(123)).unwrap();
    world.as_mut().drop::<B>(first).unwrap();
    expected.get_mut(&first).unwrap().1 = None;
    assert_rows(&mut world, &expected);

    // Singleton conversions preserve the prepared slot layout in both epoch modes.
    world.maintain_query_cache::<(Cpy<A>,)>();
    let entity = world.lookup(first).unwrap();
    let view: ViewRef<Cpy<A>> = world.view::<Cpy<A>>().into();
    assert_eq!(view.get(entity).unwrap(), A(0));
    drop(view);
    let local = world.local();
    let entity = local.lookup(first).unwrap();
    let view: LocalViewRef<Cpy<A>> = local.view::<Cpy<A>>().into();
    assert_eq!(view.get(entity).unwrap(), A(0));
}

static PANIC_PREPARE: AtomicBool = AtomicBool::new(false);
static FALLBACKS: AtomicUsize = AtomicUsize::new(0);
static PREPARED: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Copy)]
struct PanicPrepared;
impl AsQuery for PanicPrepared {
    type Query = Self;
}
impl IntoQuery for PanicPrepared {
    fn into_query(self) -> Self {
        self
    }
}
impl DefaultQuery for PanicPrepared {
    fn default_query() -> Self {
        Self
    }
}
unsafe impl Query for PanicPrepared {
    type Item<'a> = A;
    type Fetch<'a> = <Cpy<A> as Query>::Fetch<'a>;
    const MUTABLE: bool = false;
    const FETCH_CACHE_SLOTS: usize = <Cpy<A> as Query>::FETCH_CACHE_SLOTS;
    fn prepare_fetch_cache(archetype: &Archetype, slots: &mut [FetchCacheSlot]) {
        if archetype.has_component(TypeId::of::<A>())
            && PANIC_PREPARE.swap(false, Ordering::Relaxed)
        {
            // A wrong partial row must never be visible after this panic.
            Cpy::<B>::prepare_fetch_cache(archetype, slots);
            panic!("prepare aborted after writing a partial row");
        }
        Cpy::<A>::prepare_fetch_cache(archetype, slots);
    }
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        Cpy::<A>::cache_visit_archetype(archetype)
    }
    fn component_access(&self, info: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Cpy::<A>.component_access(info)
    }
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        Cpy::<A>.visit_archetype(archetype)
    }
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        unsafe { Cpy::<A>.access_archetype(archetype, f) }
    }
    unsafe fn fetch<'a>(
        &self,
        idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Self::Fetch<'a> {
        FALLBACKS.fetch_add(1, Ordering::Relaxed);
        unsafe { Cpy::<A>.fetch(idx, archetype, epoch) }
    }
    unsafe fn fetch_cached<'a>(
        &self,
        idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
        slots: &[FetchCacheSlot],
    ) -> Self::Fetch<'a> {
        PREPARED.fetch_add(1, Ordering::Relaxed);
        unsafe { Cpy::<A>.fetch_cached(idx, archetype, epoch, slots) }
    }
}
unsafe impl ImmutableQuery for PanicPrepared {}
unsafe impl SendQuery for PanicPrepared {}

#[test]
fn partial_prepare_rows_remain_unpublished_and_retry_cleanly() {
    let mut world = World::new();
    let entity = world.spawn((A(7), B(99))).id();
    PANIC_PREPARE.store(true, Ordering::Relaxed);
    assert!(
        catch_unwind(AssertUnwindSafe(
            || world.maintain_query_cache::<(PanicPrepared,)>()
        ))
        .is_err()
    );
    FALLBACKS.store(0, Ordering::Relaxed);
    PREPARED.store(0, Ordering::Relaxed);
    let entity = world.lookup(entity).unwrap();
    assert_eq!(world.view::<PanicPrepared>().get(entity), Some(A(7)));
    assert_eq!(FALLBACKS.load(Ordering::Relaxed), 1);
    assert_eq!(PREPARED.load(Ordering::Relaxed), 0);
    let entity = entity.id();
    world.maintain_query_cache::<(PanicPrepared,)>();
    let entity = world.lookup(entity).unwrap();
    assert_eq!(world.view::<PanicPrepared>().get(entity), Some(A(7)));
    assert_eq!(PREPARED.load(Ordering::Relaxed), 1);
}

static LEGACY_OPTIONAL: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Copy)]
struct LegacyOptional;
impl AsQuery for LegacyOptional {
    type Query = Self;
}
impl IntoQuery for LegacyOptional {
    fn into_query(self) -> Self {
        self
    }
}
impl DefaultQuery for LegacyOptional {
    fn default_query() -> Self {
        Self
    }
}
unsafe impl Query for LegacyOptional {
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;
    const MUTABLE: bool = false;
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
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        panic!("optional override must be preserved")
    }
    unsafe fn fetch_optional(&self, _: u32, _: &Archetype, _: EpochId) -> Option<UnitFetch> {
        LEGACY_OPTIONAL.fetch_add(1, Ordering::Relaxed);
        None
    }
}
unsafe impl ImmutableQuery for LegacyOptional {}
unsafe impl SendQuery for LegacyOptional {}

#[test]
fn zero_slot_optional_override_survives_prepared_tuple_composition() {
    for cached in [false, true] {
        let mut world = World::new();
        world.spawn_one(A(8));
        if cached {
            world.maintain_query_cache::<<((Cpy<A>, Option<LegacyOptional>),) as AsQuery>::Query>();
        }
        LEGACY_OPTIONAL.store(0, Ordering::Relaxed);
        let values: Vec<_> = world
            .view::<(Cpy<A>, Option<LegacyOptional>)>()
            .iter()
            .collect();
        assert_eq!(values, [(A(8), None)]);
        assert_eq!(LEGACY_OPTIONAL.load(Ordering::Relaxed), 1);
    }
}
