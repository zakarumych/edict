use core::{
    any::TypeId,
    sync::atomic::{AtomicUsize, Ordering},
};
use edict::{
    Access,
    archetype::Archetype,
    component::{Component, ComponentInfo},
    epoch::EpochId,
    query::{
        AsQuery, DefaultQuery, Entities, ImmutableQuery, IntoQuery, Modified, Query, Read,
        SendQuery, UnitFetch, With, Write, WriteAlias,
    },
    world::World,
};

#[derive(Component)]
struct A;
#[derive(Component)]
struct B;
#[derive(Component)]
struct Tail;

static VISITS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

#[derive(Clone, Copy)]
struct Probe<const SLOT: usize, const DYNAMIC: bool> {
    component: TypeId,
}

impl<const S: usize, const D: bool> AsQuery for Probe<S, D> {
    type Query = Self;
}
impl<const S: usize, const D: bool> IntoQuery for Probe<S, D> {
    fn into_query(self) -> Self {
        self
    }
}
impl<const S: usize, const D: bool> DefaultQuery for Probe<S, D> {
    fn default_query() -> Self {
        Self {
            component: TypeId::of::<A>(),
        }
    }
}

unsafe impl<const S: usize, const D: bool> Query for Probe<S, D> {
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;
    const MUTABLE: bool = false;
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        if D {
            None
        } else {
            Some(archetype.has_component(TypeId::of::<A>()))
        }
    }
    fn component_access(&self, _: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        VISITS[S].fetch_add(1, Ordering::Relaxed);
        archetype.has_component(if D { self.component } else { TypeId::of::<A>() })
    }
    unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(TypeId, Access)) {}
    unsafe fn visit_archetype_late(&self, _: &Archetype) -> bool {
        true
    }
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}
unsafe impl<const S: usize, const D: bool> ImmutableQuery for Probe<S, D> {}
unsafe impl<const S: usize, const D: bool> SendQuery for Probe<S, D> {}

impl edict::system::QueryArg for Probe<0, true> {
    fn new() -> Self {
        Self {
            component: TypeId::of::<B>(),
        }
    }

    fn before(&mut self, _: &World) {
        self.component = TypeId::of::<A>();
    }

    fn after(&mut self, _: &World) {
        self.component = TypeId::of::<B>();
    }
}

fn reset() {
    for v in &VISITS {
        v.store(0, Ordering::Relaxed);
    }
}
fn visits() -> [usize; 2] {
    VISITS.each_ref().map(|v| v.load(Ordering::Relaxed))
}

#[test]
fn maintained_caches_cover_public_access_and_iteration_with_fallbacks() {
    type Q = Probe<0, false>;
    type F = Probe<1, false>;
    let q = Q::default_query();
    let f = F::default_query();
    let mut world = World::new();
    let before = world.epoch();
    world.spawn((B,));
    let a = world.spawn((A,)).id();
    world.spawn((A, B));
    // Immutable world reads remain valid before explicit maintenance.
    reset();
    assert_eq!(world.view_filter_with(q, f).iter().count(), 2);
    assert!(visits().iter().all(|v| *v > 0));

    world.maintain_query_cache::<(Q,)>();
    world.maintain_query_cache::<Q>();
    world.maintain_query_cache::<F>();
    reset();
    {
        let a_entity = world
            .view::<Entities>()
            .into_iter()
            .find_map(|entity| (entity.id() == a).then_some(entity))
            .unwrap();
        let mut view = world.view_filter_with(q, f);
        let mut iter = view.iter();
        assert_eq!(iter.size_hint(), (2, Some(2)));
        assert!(iter.next().is_some());
        assert_eq!(iter.size_hint(), (1, Some(1)));
        assert_eq!(iter.fold(0, |n, _| n + 1), 1);
        assert_eq!(view.iter_mut().count(), 2);
        assert!(view.get(a_entity).is_some());
        assert!(view.map(a_entity, |_| ()).is_some());
        view.unlock();
        let mut batches = view.iter_batched(1);
        assert_eq!(batches.len(), 2);
        assert!(batches.next().is_some());
        assert_eq!(batches.len(), 1);
        assert_eq!(batches.fold(0, |n, _| n + 1), 1);
    }
    assert_eq!(world.view_filter_with(q, f).into_iter().count(), 2);
    assert_eq!(world.view_filter_with(q, f).into_iter_batched(1).count(), 2);
    assert!(world.get_with(a, q).is_ok());
    {
        let entity = world
            .view::<Entities>()
            .into_iter()
            .find_map(|entity| (entity.id() == a).then_some(entity))
            .unwrap();
        let mut one = edict::view::ViewOneValue::new(&world, entity, q, f);
        assert!(one.get().is_some());
        one.unlock();
        assert!(one.get_mut().is_some());
        one.unlock();
        assert!(one.map(|_| ()).is_some());
        assert!(one.map_mut(|_| ()).is_some());
    }
    assert_eq!(visits(), [0, 0]);

    world.maintain_query_cache::<(F,)>();
    world.maintain_query_cache::<(Q, F)>();
    world.maintain_query_cache::<(F, Q)>();
    reset();
    assert_eq!(
        world
            .view_filter_with(q, (f,))
            .extend(f)
            .filter(q)
            .iter()
            .count(),
        2
    );
    assert_eq!(visits(), [0, 0]);

    reset();
    assert_eq!(world.view_filter_with(q, f).extend(q).iter().count(), 2);
    assert!(visits()[0] > 0);

    reset();
    assert_eq!(world.view_filter_with(q, (f,)).filter(f).iter().count(), 2);
    assert_eq!(visits()[0], 0);
    assert!(visits()[1] > 0);

    // Appending a shape leaves an uncached tail until the caller maintains again.
    world.spawn((A, Tail));
    reset();
    assert_eq!(world.view_filter_with(q, f).into_iter().count(), 3);
    assert!(visits().iter().all(|v| *v > 0));
    world.maintain_query_cache::<(Q,)>();
    world.maintain_query_cache::<Q>();
    world.maintain_query_cache::<F>();
    reset();
    assert_eq!(world.view_filter_with(q, f).iter().count(), 3);
    assert_eq!(visits(), [0, 0]);

    type Dynamic = Probe<0, true>;
    world.maintain_query_cache::<(Dynamic,)>();
    reset();
    assert_eq!(world.view_with(Dynamic::default_query()).iter().count(), 3);
    assert_eq!(
        world
            .view_with(Dynamic {
                component: TypeId::of::<B>()
            })
            .iter()
            .count(),
        2
    );
    assert!(visits()[0] > 0);

    world.maintain_query_cache::<(Modified<With<A>>,)>();
    assert_eq!(
        world
            .view_with(Modified::<With<A>>::new(before))
            .iter()
            .count(),
        3
    );
    assert_eq!(
        world
            .view_with(Modified::<With<A>>::new(world.epoch()))
            .iter()
            .count(),
        0
    );

    world.maintain_query_cache::<(Read<B>,)>();
    world.maintain_query_cache::<(Write<A>,)>();
    world.maintain_query_cache::<((Write<A>, Write<B>),)>();
    let held = world.view::<&B>();
    assert_eq!(held.iter().count(), 2);
    let conflict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut writes = world.view::<(&mut A, &mut B)>();
        writes.iter_mut().count();
    }));
    assert!(conflict.is_err());
    // A was acquired before B conflicted; rollback must release that partial borrow.
    assert_eq!(world.view::<&mut A>().into_iter().count(), 3);
    drop(held);
    assert_eq!(world.view::<(&mut A, &mut B)>().into_iter().count(), 1);

    #[cfg(feature = "scheduler")]
    {
        fn query_after_before_hook(view: edict::view::View<Probe<0, true>>) {
            assert_eq!(view.iter().count(), 3);
        }
        world.maintain_query_cache::<Probe<0, true>>();
        let mut scheduler = edict::scheduler::Scheduler::new();
        scheduler.add_system(query_after_before_hook);
        scheduler.run_sequential(&mut world);
        scheduler.run_sequential(&mut world);
    }
}
