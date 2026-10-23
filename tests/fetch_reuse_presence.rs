use core::{
    any::TypeId,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use edict::{
    Access,
    archetype::Archetype,
    component::{Component, ComponentInfo},
    epoch::EpochId,
    query::{
        AsQuery, DefaultQuery, EpochOf, ImmutableQuery, IntoQuery, Modified, Not, Query, SendQuery,
        UnitFetch, With, WithEpoch, WriteAlias,
    },
    world::World,
};

#[derive(Component)]
struct Value(u32);

static EARLY_CALLS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];
static LATE_CALLS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];
#[derive(Clone, Copy)]
struct Stable<const PRESENT: bool>;
impl<const P: bool> AsQuery for Stable<P> {
    type Query = Self;
}
impl<const P: bool> IntoQuery for Stable<P> {
    fn into_query(self) -> Self {
        self
    }
}
impl<const P: bool> DefaultQuery for Stable<P> {
    fn default_query() -> Self {
        Self
    }
}
unsafe impl<const P: bool> Query for Stable<P> {
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;
    const MUTABLE: bool = false;
    const SUPPORTS_FETCH_REUSE: bool = true;
    const FETCH_REUSE_MATCHES_STABLE: bool = true;
    fn cache_visit_archetype(_: &Archetype) -> Option<bool> {
        Some(P)
    }
    fn component_access(&self, _: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }
    fn visit_archetype(&self, _: &Archetype) -> bool {
        EARLY_CALLS[P as usize].fetch_add(1, Ordering::Relaxed);
        P
    }
    unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(TypeId, Access)) {}
    unsafe fn visit_archetype_late(&self, _: &Archetype) -> bool {
        LATE_CALLS[P as usize].fetch_add(1, Ordering::Relaxed);
        assert!(P);
        true
    }
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}
unsafe impl<const P: bool> ImmutableQuery for Stable<P> {}
unsafe impl<const P: bool> SendQuery for Stable<P> {}

fn calls() -> ([usize; 2], [usize; 2]) {
    (
        EARLY_CALLS.each_ref().map(|v| v.load(Ordering::Relaxed)),
        LATE_CALLS.each_ref().map(|v| v.load(Ordering::Relaxed)),
    )
}

#[test]
fn stable_some_none_nested_options_and_tuples_skip_repeated_matching() {
    let mut world = World::new();
    let id = world.spawn((Value(0),)).id();
    let view = world.view::<(Option<Stable<false>>, Option<Stable<true>>)>();
    assert_eq!(view.try_get(id).unwrap(), (None, Some(())));
    let warm = calls();
    for _ in 0..300 {
        assert_eq!(view.try_get(id).unwrap(), (None, Some(())));
    }
    assert_eq!(calls(), warm);
    drop(view);
    let view = world.view::<(Option<Option<Stable<false>>>, Option<Option<Stable<true>>>)>();
    assert_eq!(view.try_get(id).unwrap(), (Some(None), Some(Some(()))));
    let warm = calls();
    for _ in 0..300 {
        assert_eq!(view.try_get(id).unwrap(), (Some(None), Some(Some(()))));
    }
    assert_eq!(calls(), warm);
}

static SWITCH: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];
#[derive(Clone, Copy)]
struct Dynamic<const EARLY: bool>;
impl<const E: bool> AsQuery for Dynamic<E> {
    type Query = Self;
}
impl<const E: bool> IntoQuery for Dynamic<E> {
    fn into_query(self) -> Self {
        self
    }
}
impl<const E: bool> DefaultQuery for Dynamic<E> {
    fn default_query() -> Self {
        Self
    }
}
unsafe impl<const E: bool> Query for Dynamic<E> {
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;
    const MUTABLE: bool = false;
    const SUPPORTS_FETCH_REUSE: bool = true;
    fn cache_visit_archetype(_: &Archetype) -> Option<bool> {
        None
    }
    fn component_access(&self, _: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }
    fn visit_archetype(&self, _: &Archetype) -> bool {
        !E || SWITCH[E as usize].load(Ordering::Relaxed)
    }
    unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(TypeId, Access)) {}
    unsafe fn visit_archetype_late(&self, _: &Archetype) -> bool {
        if E {
            assert!(SWITCH[E as usize].load(Ordering::Relaxed));
            true
        } else {
            SWITCH[E as usize].load(Ordering::Relaxed)
        }
    }
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}
unsafe impl<const E: bool> ImmutableQuery for Dynamic<E> {}
unsafe impl<const E: bool> SendQuery for Dynamic<E> {}

fn dynamic<const E: bool>() {
    assert!(!Dynamic::<E>::FETCH_REUSE_MATCHES_STABLE);
    assert!(!Not::<Dynamic<E>>::FETCH_REUSE_MATCHES_STABLE);
    let mut world = World::new();
    let id = world.spawn((Value(0),)).id();
    for initial in [false, true] {
        SWITCH[E as usize].store(initial, Ordering::Relaxed);
        let view = world.view::<Option<Dynamic<E>>>();
        assert_eq!(view.try_get(id).unwrap().is_some(), initial);
        for on in [true, false, false, true, false, true] {
            SWITCH[E as usize].store(on, Ordering::Relaxed);
            assert_eq!(view.try_get(id).unwrap().is_some(), on);
        }
    }
}

#[test]
fn reusable_dynamic_early_and_late_acceptance_still_changes_presence() {
    dynamic::<true>();
    dynamic::<false>();
}

#[test]
fn stable_mutable_some_preserves_previous_epochs_and_modification_tracking() {
    let mut world = World::new();
    let id = world.spawn((Value(0),)).id();
    let mut previous = world.get::<EpochOf<Value>>(id).unwrap();
    let cutoff = world.epoch();
    {
        let mut view = world.view::<Option<WithEpoch<&mut Value>>>();
        for _ in 0..600 {
            let (value, old) = view.try_get_mut(id).unwrap().unwrap();
            assert_eq!(old, previous);
            value.0 += 1;
            previous = world.epoch();
            assert!(previous.after(old));
        }
    }
    assert_eq!(world.get::<&Value>(id).unwrap().0, 600);
    assert_eq!(
        world
            .view_with(Modified::<With<Value>>::new(cutoff))
            .iter()
            .count(),
        1
    );
}
