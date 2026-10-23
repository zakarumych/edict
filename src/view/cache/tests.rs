use core::{
    any::TypeId,
    cell::Cell,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::{
    boxed::Box,
    panic::{AssertUnwindSafe, catch_unwind},
};

use crate::{
    Access,
    archetype::Archetype,
    component::{Component, ComponentInfo},
    epoch::EpochId,
    query::{AsQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias},
    view::LocalViewRef,
    world::World,
};

struct A;
impl Component for A {}
struct B;
impl Component for B {}

#[derive(Default)]
struct Counts {
    creates: AtomicUsize,
    reuses: AtomicUsize,
    drops: AtomicUsize,
    reject: AtomicBool,
    panic: AtomicBool,
    panic_at: AtomicUsize,
    drop_order: std::sync::Mutex<std::vec::Vec<bool>>,
}

#[derive(Clone, Copy)]
struct Probe<const REUSE: bool, const MUTABLE: bool>(&'static Counts);

impl<const R: bool, const M: bool> AsQuery for Probe<R, M> {
    type Query = Self;
}
impl<const R: bool, const M: bool> IntoQuery for Probe<R, M> {
    fn into_query(self) -> Self {
        self
    }
}

struct ProbeFetch {
    counts: &'static Counts,
    epoch: EpochId,
    stage: u8,
    count_drop: bool,
    retained: bool,
}
impl Drop for ProbeFetch {
    fn drop(&mut self) {
        if self.count_drop {
            self.counts.drops.fetch_add(1, Ordering::Relaxed);
        } else {
            self.counts.drop_order.lock().unwrap().push(self.retained);
        }
    }
}
unsafe impl<'a> Fetch<'a> for ProbeFetch {
    type Item = EpochId;
    fn dangling() -> Self {
        static COUNTS: Counts = Counts {
            creates: AtomicUsize::new(0),
            reuses: AtomicUsize::new(0),
            drops: AtomicUsize::new(0),
            reject: AtomicBool::new(false),
            panic: AtomicBool::new(false),
            panic_at: AtomicUsize::new(0),
            drop_order: std::sync::Mutex::new(std::vec::Vec::new()),
        };
        Self {
            counts: &COUNTS,
            epoch: EpochId::start(),
            stage: u8::MAX,
            count_drop: false,
            retained: false,
        }
    }
    unsafe fn visit_chunk(&mut self, _: u32) -> bool {
        panic_at(self.counts, 5);
        assert_eq!(self.stage, 0);
        self.stage = 1;
        true
    }
    unsafe fn touch_chunk(&mut self, _: u32) {
        panic_at(self.counts, 6);
        assert_eq!(self.stage, 1);
        self.stage = 2;
    }
    unsafe fn visit_item(&mut self, _: u32) -> bool {
        panic_at(self.counts, 7);
        assert_eq!(self.stage, 2);
        self.stage = 3;
        true
    }
    unsafe fn get_item(&mut self, _: u32) -> EpochId {
        panic_at(self.counts, 8);
        assert_eq!(self.stage, 3);
        self.stage = 4;
        self.epoch
    }
}

std::thread_local! {
    static ON_REUSE: Cell<Option<fn()>> = const { Cell::new(None) };
    static REENTER_VIEW: Cell<*const ()> = const { Cell::new(core::ptr::null()) };
    static REENTER_ENTITY: Cell<Option<crate::entity::EntityId>> = const { Cell::new(None) };
}

// Probe fetches retain no component references. Reused handles contain only
// independent protocol state, copied epochs and shared atomic test counters.
unsafe impl<const R: bool, const M: bool> Query for Probe<R, M> {
    type Item<'a> = EpochId;
    type Fetch<'a> = ProbeFetch;
    const MUTABLE: bool = M;
    const FILTERS_ENTITIES: bool = false;
    const SUPPORTS_FETCH_REUSE: bool = R;
    fn cache_visit_archetype(_: &Archetype) -> Option<bool> {
        Some(true)
    }
    fn component_access(&self, _: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }
    fn visit_archetype(&self, _: &Archetype) -> bool {
        panic_at(self.0, 1);
        true
    }
    unsafe fn visit_archetype_late(&self, _: &Archetype) -> bool {
        panic_at(self.0, 2);
        !self.0.reject.load(Ordering::Relaxed)
    }
    unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(TypeId, Access)) {}
    unsafe fn fetch(&self, _: u32, _: &Archetype, epoch: EpochId) -> ProbeFetch {
        panic_at(self.0, 3);
        self.0.creates.fetch_add(1, Ordering::Relaxed);
        ProbeFetch {
            counts: self.0,
            epoch,
            stage: 0,
            count_drop: !R,
            retained: true,
        }
    }
    unsafe fn reuse_fetch<'cached, 'a>(
        &self,
        cached: &mut ProbeFetch,
        _: u32,
        _: &'a Archetype,
        epoch: EpochId,
    ) -> ProbeFetch {
        panic_at(self.0, 4);
        self.0.reuses.fetch_add(1, Ordering::Relaxed);
        assert!(
            !self.0.panic.swap(false, Ordering::Relaxed),
            "Probe reuse panic"
        );
        ON_REUSE.with(|callback| {
            if let Some(callback) = callback.take() {
                callback();
            }
        });
        cached.epoch = epoch;
        ProbeFetch {
            counts: self.0,
            epoch,
            stage: 0,
            count_drop: false,
            retained: false,
        }
    }
}
unsafe impl<const R: bool> ImmutableQuery for Probe<R, false> {}
unsafe impl<const R: bool, const M: bool> SendQuery for Probe<R, M> {}

#[test]
fn last_archetype_hit_reuses_fetch_and_runs_protocol_with_fresh_epochs() {
    let mut world = World::new();
    let ids: std::vec::Vec<_> = (0..600).map(|_| world.spawn((A,)).id()).collect();
    let other = world.spawn((B,)).id();
    let counts = Box::leak(Box::new(Counts::default()));
    let local = world.local();
    let mut view = local.view_with(Probe::<true, true>(counts));
    let mut previous = local.epoch();
    for id in [ids[0], ids[0], ids[599], other, ids[0]] {
        let epoch = view.try_get_mut(id).unwrap();
        assert!(epoch.after(previous));
        assert_eq!(epoch, local.epoch());
        previous = epoch;
    }
    assert_eq!(counts.creates.load(Ordering::Relaxed), 3);
    assert_eq!(counts.reuses.load(Ordering::Relaxed), 5);
    assert_eq!(counts.drops.load(Ordering::Relaxed), 0);
}

#[test]
fn immutable_indexing_supports_reentry_and_unwind_clears_cache() {
    let mut world = World::new();
    let id = world.spawn((A,)).id();
    let counts = Box::leak(Box::new(Counts::default()));
    let view = world.local().view_with(Probe::<true, false>(counts));
    let before = view.try_get(id).unwrap();
    assert_eq!(view.try_get(id).unwrap(), before);
    assert_eq!(counts.creates.load(Ordering::Relaxed), 1);

    fn reenter() {
        REENTER_VIEW.with(|slot| {
            // The outer call holds this shared view alive until callback returns.
            let view = unsafe { &*slot.get().cast::<LocalViewRef<'_, (Probe<true, false>,)>>() };
            let id = REENTER_ENTITY.with(|slot| slot.get().unwrap());
            view.try_get(id).unwrap();
        });
    }
    REENTER_VIEW.with(|slot| slot.set((&view as *const _) as *const ()));
    REENTER_ENTITY.with(|slot| slot.set(Some(id)));
    ON_REUSE.with(|callback| callback.set(Some(reenter)));
    assert_eq!(view.try_get(id).unwrap(), before);
    REENTER_VIEW.with(|slot| slot.set(core::ptr::null()));
    counts.panic.store(true, Ordering::Relaxed);
    assert!(catch_unwind(AssertUnwindSafe(|| view.try_get(id))).is_err());
    assert!(view.last_fetches.is_empty());
    assert_eq!(view.try_get(id).unwrap(), before);
    assert_eq!(counts.creates.load(Ordering::Relaxed), 3);
}

#[test]
fn unlock_iteration_and_temporary_maps_discard_retained_handles() {
    let mut world = World::new();
    let id = world.spawn((A,)).id();
    let counts = Box::leak(Box::new(Counts::default()));
    let mut view = world.local().view_with(Probe::<true, false>(counts));
    view.try_get(id).unwrap();
    view.unlock();
    view.try_get(id).unwrap();
    assert_eq!(counts.creates.load(Ordering::Relaxed), 2);
    drop(view.iter());
    assert!(view.last_fetches.is_empty());
    view.try_get(id).unwrap();
    assert_eq!(counts.creates.load(Ordering::Relaxed), 3);
    view.unlock();
    view.try_map(id, |_| ()).unwrap();
    assert!(view.last_fetches.is_empty());
    view.try_get(id).unwrap();
    assert_eq!(counts.creates.load(Ordering::Relaxed), 5);
}

#[test]
fn unsupported_fetch_drop_and_late_rejection_preserve_indexing_behavior() {
    let mut world = World::new();
    let id = world.spawn((A,)).id();
    let counts = Box::leak(Box::new(Counts::default()));
    let local = world.local();
    let mut view = local.view_with(Probe::<false, true>(counts));
    for expected in 1..=3 {
        view.try_get_mut(id).unwrap();
        assert_eq!(counts.creates.load(Ordering::Relaxed), expected);
        assert_eq!(counts.drops.load(Ordering::Relaxed), expected);
        assert_eq!(counts.reuses.load(Ordering::Relaxed), 0);
    }
    let before = local.epoch();
    counts.reject.store(true, Ordering::Relaxed);
    assert!(view.try_get_mut(id).is_err());
    assert_eq!(local.epoch(), before);
    assert_eq!(counts.creates.load(Ordering::Relaxed), 3);
}

#[test]
fn mutable_filter_and_query_refresh_their_own_retained_fetches() {
    let mut world = World::new();
    let id = world.spawn((A,)).id();
    let query_counts = Box::leak(Box::new(Counts::default()));
    let filter_counts = Box::leak(Box::new(Counts::default()));
    let local = world.local();
    let mut view = local
        .view_with(Probe::<true, false>(query_counts))
        .filter(Probe::<true, true>(filter_counts));
    let mut previous = local.epoch();
    for _ in 0..3 {
        let epoch = view.try_get_mut(id).unwrap();
        assert!(epoch.after(previous));
        assert_eq!(local.epoch(), epoch);
        previous = epoch;
    }
    assert_eq!(query_counts.creates.load(Ordering::Relaxed), 1);
    assert_eq!(filter_counts.creates.load(Ordering::Relaxed), 1);
    assert_eq!(query_counts.reuses.load(Ordering::Relaxed), 3);
    assert_eq!(filter_counts.reuses.load(Ordering::Relaxed), 3);
}

fn panic_at(counts: &Counts, stage: usize) {
    if counts
        .panic_at
        .compare_exchange(stage, 0, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        panic!("Probe phase {stage}");
    }
}

fn nested_view(f: impl FnOnce(&LocalViewRef<'_, (Probe<true, false>,)>, crate::entity::EntityId)) {
    REENTER_VIEW.with(|slot| {
        // The outer access keeps this exact shared view and its world alive.
        let view = unsafe { &*slot.get().cast::<LocalViewRef<'_, (Probe<true, false>,)>>() };
        let id = REENTER_ENTITY.with(|slot| slot.get().unwrap());
        f(view, id);
    });
}

#[test]
fn nested_iteration_defers_clear_until_short_fetch_is_dropped() {
    let mut world = World::new();
    let id = world.spawn((A,)).id();
    let counts = Box::leak(Box::new(Counts::default()));
    let view = world.local().view_with(Probe::<true, false>(counts));
    view.try_get(id).unwrap();
    counts.drop_order.lock().unwrap().clear();
    REENTER_VIEW.with(|slot| slot.set((&view as *const _) as *const ()));
    REENTER_ENTITY.with(|slot| slot.set(Some(id)));
    fn iterate() {
        nested_view(|view, _| {
            drop(view.iter());
            assert_eq!(
                view.last_fetches.state.get(),
                super::CacheState::PendingClear
            );
        });
    }
    ON_REUSE.with(|slot| slot.set(Some(iterate)));
    view.try_get(id).unwrap();
    REENTER_VIEW.with(|slot| slot.set(core::ptr::null()));
    assert!(view.last_fetches.is_empty());
    assert_eq!(*counts.drop_order.lock().unwrap(), [false, true]);
    view.try_get(id).unwrap();
    assert_eq!(counts.creates.load(Ordering::Relaxed), 2);
}

#[test]
fn nested_temporary_map_defers_clear_and_uses_uncached_fetch() {
    let mut world = World::new();
    let id = world.spawn((A,)).id();
    let counts = Box::leak(Box::new(Counts::default()));
    let view = world.local().view_with(Probe::<true, false>(counts));
    view.try_get(id).unwrap();
    counts.drop_order.lock().unwrap().clear();
    REENTER_VIEW.with(|slot| slot.set((&view as *const _) as *const ()));
    REENTER_ENTITY.with(|slot| slot.set(Some(id)));
    fn map() {
        nested_view(|view, id| {
            view.try_map(id, |_| ()).unwrap();
            assert_eq!(
                view.last_fetches.state.get(),
                super::CacheState::PendingClear
            );
        });
    }
    ON_REUSE.with(|slot| slot.set(Some(map)));
    view.try_get(id).unwrap();
    REENTER_VIEW.with(|slot| slot.set(core::ptr::null()));
    assert!(view.last_fetches.is_empty());
    assert_eq!(*counts.drop_order.lock().unwrap(), [true, false, true]);
    view.try_get(id).unwrap();
    assert_eq!(counts.creates.load(Ordering::Relaxed), 3);
}

#[test]
fn panics_in_each_fetch_phase_clear_the_entry_and_restore_idle() {
    for phase in 1..=8 {
        let mut world = World::new();
        let a = world.spawn((A,)).id();
        let b = world.spawn((B,)).id();
        let counts = Box::leak(Box::new(Counts::default()));
        let view = world.local().view_with(Probe::<true, false>(counts));
        view.try_get(a).unwrap();
        counts.panic_at.store(phase, Ordering::Relaxed);
        let target = if phase == 3 { b } else { a };
        assert!(
            catch_unwind(AssertUnwindSafe(|| view.try_get(target))).is_err(),
            "phase {phase}"
        );
        assert!(view.last_fetches.is_empty());
        view.try_get(a).unwrap();
    }
}

#[test]
fn releasing_an_active_entry_panics_without_changing_its_state() {
    let cache = super::FetchCache::<()>::new();
    let access = cache.enter().unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| cache.assert_idle())).is_err());
    assert_eq!(cache.state.get(), super::CacheState::Busy);
    drop(access);
    assert!(cache.is_empty());
}
