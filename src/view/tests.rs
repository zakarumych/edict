use core::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::{
    epoch::EpochId,
    query::{AsQuery, IntoQuery, QueryCache, SendQuery, UnitFetch, With, WriteAlias},
};

static VISITS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

#[derive(Clone, Copy)]
struct Counted<const ID: usize>;

impl<const ID: usize> AsQuery for Counted<ID> {
    type Query = Self;
}

impl<const ID: usize> IntoQuery for Counted<ID> {
    fn into_query(self) -> Self {
        self
    }
}

unsafe impl<const ID: usize> Query for Counted<ID> {
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;
    const MUTABLE: bool = false;

    fn cache_visit_archetype(_: &Archetype) -> Option<bool> {
        Some(ID == 0)
    }

    fn component_access(&self, _: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }

    fn visit_archetype(&self, _: &Archetype) -> bool {
        VISITS[ID].fetch_add(1, Ordering::Relaxed);
        ID == 0
    }

    unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(core::any::TypeId, Access)) {}

    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<const ID: usize> SendQuery for Counted<ID> {}

type Source<'a> = ViewValue<'a, (Counted<0>,), (Counted<1>,), StaticallyBorrowed, Extensible>;

fn source<'a>(world: &'a World, caches: &'a QueryCache) -> Source<'a> {
    // These queries access no component data.
    let mut view = unsafe {
        ViewValue::new_unchecked(
            world,
            (Counted::<0>,),
            (Counted::<1>,),
            StaticallyBorrowed,
            Extensible,
        )
    };
    view.cache = caches;
    view.query = caches.get(*view.query);
    view.filter = caches.get(*view.filter);
    view
}

fn check_caches<Q: Query, F: Query, B: BorrowState, E>(
    view: &mut ViewValue<'_, Q, F, B, E>,
    expected_query: bool,
    expected_visits: [usize; 2],
) {
    for visits in &VISITS {
        visits.store(0, Ordering::Relaxed);
    }
    let archetype = &view.archetypes[0];
    assert_eq!(view.query.visit_archetype(archetype, 0), expected_query);
    assert!(!view.filter.visit_archetype(archetype, 0));
    assert_eq!(
        VISITS.each_ref().map(|v| v.load(Ordering::Relaxed)),
        expected_visits
    );
}

#[test]
fn view_cache_propagation_preserves_conversions_and_looks_up_extensions() {
    let world = World::new();
    let mut caches = QueryCache::new();
    caches.init::<(Counted<0>,)>(world.archetypes());
    caches.init::<(Counted<1>,)>(world.archetypes());
    caches.init::<(Counted<0>, With<u64>)>(world.archetypes());
    caches.init::<(Counted<1>, With<u64>)>(world.archetypes());
    caches.init::<((Counted<0>,), With<u64>)>(world.archetypes());

    macro_rules! conversion {
        ($query:ty, $state:ty, $extensible:ty) => {{
            let mut view: ViewValue<'_, $query, (Counted<1>,), $state, $extensible> =
                source(&world, &caches).into();
            assert!(core::ptr::eq(view.cache, &caches));
            check_caches(&mut view, true, [0, 0]);
        }};
    }
    conversion!((Counted<0>,), RuntimeBorrowState, Extensible);
    conversion!((Counted<0>,), StaticallyBorrowed, NonExtensible);
    conversion!((Counted<0>,), RuntimeBorrowState, NonExtensible);
    conversion!(Counted<0>, StaticallyBorrowed, Extensible);
    conversion!(Counted<0>, RuntimeBorrowState, Extensible);
    conversion!(Counted<0>, StaticallyBorrowed, NonExtensible);
    conversion!(Counted<0>, RuntimeBorrowState, NonExtensible);

    let mut wrapped = source(&world, &caches).into_tuple_query();
    check_caches(&mut wrapped, true, [0, 0]);
    let mut unwrapped: Source<'_> = wrapped.into();
    check_caches(&mut unwrapped, true, [0, 0]);
    assert!(core::ptr::eq(unwrapped.cache, &caches));

    let mut cloned = unwrapped.clone();
    assert!(core::ptr::eq(cloned.cache, &caches));
    {
        let mut locked = cloned.lock();
        assert!(core::ptr::eq(locked.cache, &caches));
        check_caches(&mut locked, true, [0, 0]);
    }

    let converted: ViewValue<'_, (Counted<0>,), (Counted<1>,), RuntimeBorrowState, Extensible> =
        unwrapped.into();
    let mut extended = converted.extend(With::<u64>::default());
    check_caches(&mut extended, false, [0, 0]);
    let mut filtered = source(&world, &caches).filter(With::<u64>::default());
    check_caches(&mut filtered, true, [0, 0]);
    let mut wrapped_extended = source(&world, &caches)
        .into_tuple_query()
        .extend(With::<u64>::default());
    check_caches(&mut wrapped_extended, false, [0, 0]);

    let mut extended = source(&world, &caches).extend(With::<u32>::default());
    check_caches(&mut extended, false, [1, 0]);
    let mut filtered = source(&world, &caches).filter(With::<u32>::default());
    check_caches(&mut filtered, true, [0, 1]);
}

#[test]
fn local_epoch_source_survives_extensions_locking_iteration_and_single_views() {
    use crate::{
        component::Component,
        query::{DefaultQuery, Write},
    };
    #[derive(Component)]
    struct LocalValue(u32);
    let mut world = World::new();
    let entity = world.spawn((LocalValue(0),)).id();
    let start = world.epoch();
    let alive = world
        .view::<crate::query::Entities>()
        .into_iter()
        .next()
        .unwrap();

    let end;
    {
        // The mutable world owner performs no atomic epoch operation while any
        // local source, view, item, or iterator is in use.
        let epochs = unsafe { LocalEpochCounter::from_atomic(world.epoch_counter()) };
        let source = unsafe {
            ViewValue::new_with_epochs(&world, (), (), StaticallyBorrowed, Extensible, epochs)
        };
        let mut view = source
            .extend_default::<&mut LocalValue>()
            .with::<LocalValue>();
        view.try_get_mut(entity).unwrap().0 += 1;
        let first = epochs.current();
        view.try_get_mut(entity).unwrap().0 += 1;
        assert!(epochs.current().after(first));
        {
            let mut locked = view.lock();
            for value in locked.iter_mut() {
                value.0 += 1;
            }
            for batch in locked.iter_batched_mut(1) {
                batch[0].0 += 1;
            }
        }
        drop(view);
        let mut one = unsafe {
            ViewOneValue::new_with_epochs(
                &world,
                alive,
                Write::<LocalValue>::default_query(),
                (),
                epochs,
            )
        };
        one.get_mut().unwrap().0 += 1;
        drop(one);
        end = epochs.current();
        assert!(end.after(start));
    }
    assert_eq!(world.epoch(), end);
    assert_eq!(world.get::<&LocalValue>(entity).unwrap().0, 5);
}

#[test]
fn atomic_iterators_keep_send_and_sync_capabilities() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ViewIter<'static, (), ()>>();
    send_sync::<ViewBatchIter<'static, (), ()>>();
}
