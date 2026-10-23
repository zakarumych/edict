use super::*;
use crate::{component::Component, relation::*, world::World};

#[derive(Component, Clone, Copy)]
struct A;
#[derive(Component)]
struct B;

fn assert_cached<Q: Query>(query: Q, archetype: &Archetype) {
    assert_eq!(
        Q::cache_visit_archetype(archetype),
        Some(query.visit_archetype(archetype))
    );
}

#[test]
fn cache_visit_archetype_components_and_wrappers() {
    let mut world = World::new();
    world.spawn((A,));
    world.spawn((B,));
    world.spawn((A, B));
    for archetype in world.archetypes() {
        macro_rules! check {
            ($($ty:ty),+ $(,)?) => {$(
                assert_cached(<<$ty as AsQuery>::Query as DefaultQuery>::default_query(), archetype);
            )+};
        }
        check!(
            (),
            Entities,
            &A,
            &mut A,
            Alt<A>,
            Cpy<A>,
            With<A>,
            Without<A>,
            Option<&A>,
            WithEpoch<&A>,
            WithEpoch<&mut A>,
            WithEpoch<Alt<A>>,
            EpochOf<A>,
            BorrowAny<&A>,
            BorrowAny<&mut A>,
            BorrowAll<&A>,
            BorrowAll<&mut A>,
            (With<A>, Without<B>),
            Or<(With<A>, With<B>)>,
            And<(With<A>, With<B>)>,
            Xor<(With<A>, With<B>)>
        );
        let epoch = world.epoch();
        assert_cached(Modified::<Read<A>>::new(epoch), archetype);
        assert_cached(Modified::<Write<A>>::new(epoch), archetype);
        assert_cached(Modified::<Alt<A>>::new(epoch), archetype);
        assert_cached(Modified::<Cpy<A>>::new(epoch), archetype);
        assert_cached(Modified::<With<A>>::new(epoch), archetype);
        assert_cached(Not(Modified::<With<A>>::new(epoch)), archetype);
        // Entity filtering keeps an archetype eligible even when both XOR arms match.
        assert_cached(
            Xor::<(Modified<With<A>>, With<B>)>::new(
                Modified::<With<A>>::new(epoch),
                With::<B>::default(),
            ),
            archetype,
        );
    }
}

#[derive(Clone, Copy, Relation)]
struct R;
#[derive(Clone, Copy, Relation)]
#[edict(symmetric)]
struct S;
#[derive(Clone, Copy, Relation)]
#[edict(exclusive)]
struct E;

#[test]
fn cache_visit_archetype_relations_ignore_instance_targets() {
    let mut world = World::new();
    let origin = world.spawn_empty().id();
    let target = world.spawn_empty().id();
    world.insert_relation(origin, R, target).unwrap();
    world.insert_relation(origin, S, target).unwrap();
    world.insert_relation(origin, E, target).unwrap();
    for archetype in world.archetypes() {
        macro_rules! check {
            ($($ty:ty),+ $(,)?) => {$(
                assert_cached(<<$ty as AsQuery>::Query as DefaultQuery>::default_query(), archetype);
            )+};
        }
        check!(
            Relates<With<R>>,
            Relates<&R>,
            Relates<&mut R>,
            Related<With<R>>,
            Related<&R>,
            Related<&mut R>,
            Related<With<S>>,
            Related<&S>,
            Related<&mut S>,
            FilterRelated<R>,
            FilterRelated<S>,
            FilterRelates<R>,
            RelatesExclusive<With<E>>,
            RelatesExclusive<&E>,
            RelatesExclusive<&mut E>
        );
        for entity in [origin, target] {
            let query = RelatesTo::<Read<R>>::new(entity);
            assert_cached(query, archetype);
            assert_cached(RelatesTo::<Write<R>>::new(entity), archetype);
            assert_cached(FilterRelatesTo::<R>::new(entity), archetype);
            assert_cached(FilterRelatedBy::<R>::new(entity), archetype);
            assert_cached(FilterRelatedBy::<S>::new(entity), archetype);
            assert_cached(Modified::with_query(world.epoch(), query), archetype);
        }
    }
}

#[test]
fn cache_visit_archetype_dynamic_borrows_and_tuple_rejection() {
    let a_info = ComponentInfo::of::<A>();
    let archetype = Archetype::new(core::iter::once(&a_info));
    let present = BorrowOne::<Read<A>>::new(TypeId::of::<A>());
    let absent = BorrowOne::<Read<A>>::new(TypeId::of::<B>());
    assert_eq!(
        BorrowOne::<Read<A>>::cache_visit_archetype(&archetype),
        None
    );
    assert!(present.visit_archetype(&archetype));
    assert!(!absent.visit_archetype(&archetype));
    assert_eq!(
        Not::<BorrowOne<Read<A>>>::cache_visit_archetype(&archetype),
        None
    );
    assert_eq!(
        <(BorrowOne<Read<A>>, With<B>)>::cache_visit_archetype(&archetype),
        Some(false)
    );
    assert_eq!(
        <(With<B>, BorrowOne<Read<A>>)>::cache_visit_archetype(&archetype),
        Some(false)
    );
    assert_eq!(
        <(BorrowOne<Read<A>>, With<A>)>::cache_visit_archetype(&archetype),
        None
    );
    assert_eq!(
        Or::<(BorrowOne<Read<A>>, With<B>)>::cache_visit_archetype(&archetype),
        None
    );
    let mut caches = QueryCache::new();
    caches.init::<BorrowOne<Read<A>>>(core::slice::from_ref(&archetype));
    let cache = caches.get(present);
    let absent_cache = caches.get(absent);
    assert!(cache.visit_archetype(&archetype, 0));
    assert!(!absent_cache.visit_archetype(&archetype, 0));
    assert!(cache.visit_archetype(&archetype, 0));
    assert!((*cache).visit_archetype(&archetype));
    assert!(!(*absent_cache).visit_archetype(&archetype));
    assert!(!QueryCacheRef::empty(absent).visit_archetype(&archetype, 0));
    for owned in [cache, absent_cache] {
        let expected = (*owned).visit_archetype(&archetype);
        let tuple = owned.into_tuple();
        assert_eq!(tuple.visit_archetype(&archetype, 0), expected);
        assert_eq!(tuple.bits, owned.bits);
        let unwrapped = tuple.unwrap_tuple();
        assert_eq!(unwrapped.visit_archetype(&archetype, 0), expected);
        assert_eq!(unwrapped.bits, owned.bits);
    }
    assert_eq!(cache.bits, &[0]);
}

#[test]
fn cache_visit_archetype_packed_states_cross_word_boundary() {
    let a_info = ComponentInfo::of::<A>();
    let present = Archetype::new(core::iter::once(&a_info));
    let absent = Archetype::new(core::iter::empty());
    let archetypes: alloc::vec::Vec<_> = (0..64)
        .map(|idx| {
            if idx == 31 || idx == 63 {
                Archetype::new(core::iter::once(&a_info))
            } else {
                Archetype::new(core::iter::empty())
            }
        })
        .collect();
    let mut caches = QueryCache::new();
    caches.init::<With<A>>(&archetypes[..32]);
    caches.init::<With<A>>(&archetypes);
    let query = With::<A>::default();
    let cache = caches.get(query);
    for _ in 0..2 {
        assert!(!cache.visit_archetype(&absent, 0));
        assert!(cache.visit_archetype(&present, 31));
        assert!(!cache.visit_archetype(&absent, 32));
        assert!(cache.visit_archetype(&present, 63));
    }
    let expected = 0x1555_5555_5555_5555 | (2 << 62);
    assert_eq!(cache.bits, &[expected, expected]);
}

#[test]
fn cache_visit_archetype_preserves_late_epoch_filtering() {
    let mut world = World::new();
    let before = world.epoch();
    world.spawn((A,));
    let after = world.epoch();
    let archetype = world
        .archetypes()
        .iter()
        .find(|a| a.has_component(TypeId::of::<A>()))
        .unwrap();
    let changed = Modified::<Read<A>>::new(before);
    let unchanged = Modified::<Read<A>>::new(after);
    let mut caches = QueryCache::new();
    caches.init::<Modified<Read<A>>>(world.archetypes());
    let cache = caches.get(changed);
    let unchanged_cache = caches.get(unchanged);
    let idx = world
        .archetypes()
        .iter()
        .position(|a| core::ptr::eq(a, archetype))
        .unwrap();
    assert!(cache.visit_archetype(archetype, idx as u32));
    assert!(unchanged_cache.visit_archetype(archetype, idx as u32));
    assert_eq!(cache.after_epoch(), changed.after_epoch());
    assert_eq!(unchanged_cache.after_epoch(), unchanged.after_epoch());
    // This test holds the world exclusively and only reads component epochs.
    unsafe {
        changed.access_archetype(archetype, |_, _| {});
        assert!(changed.visit_archetype_late(archetype));
        unchanged.access_archetype(archetype, |_, _| {});
        assert!(!unchanged.visit_archetype_late(archetype));
    }
    assert_eq!((cache.bits[idx / 32] >> (2 * (idx % 32))) & 3, 2);
}

#[test]
fn cache_visit_archetype_boolean_short_circuits_known_operands() {
    let a_info = ComponentInfo::of::<A>();
    let archetype = Archetype::new(core::iter::once(&a_info));
    type Dynamic = BorrowOne<Read<A>>;
    macro_rules! check {
        ($ty:ty, $expected:expr) => {
            assert_eq!(
                <$ty>::cache_visit_archetype(&archetype),
                $expected,
                stringify!($ty)
            );
        };
    }
    check!(And<(Dynamic, With<B>)>, Some(false));
    check!(And<(With<B>, Dynamic)>, Some(false));
    check!(Or<(Dynamic, With<A>)>, Some(true));
    check!(Or<(With<A>, Dynamic)>, Some(true));
    check!(And<(Dynamic, With<A>)>, None);
    check!(Or<(Dynamic, With<B>)>, None);
    check!(Xor<(Dynamic, With<A>)>, None);
    check!(Xor<(With<A>, Dynamic)>, None);
    check!(Xor<(Dynamic, With<A>, With<A>)>, Some(false));
    check!(Xor<(With<A>, With<A>, Dynamic)>, Some(false));
    check!(And<(Dynamic, Dynamic)>, None);
    check!(Or<(Dynamic, Dynamic)>, None);
    check!(Xor<(Dynamic, Dynamic)>, None);
    check!(And<(With<A>, With<A>)>, Some(true));
    check!(And<(With<A>, With<B>)>, Some(false));
    check!(Or<(With<A>, With<B>)>, Some(true));
    check!(Or<(With<B>, With<B>)>, Some(false));
    check!(Xor<(With<A>, With<B>)>, Some(true));
    check!(Xor<(With<A>, With<A>)>, Some(false));
    check!(Xor<(With<B>, With<B>)>, Some(false));
}

#[test]
fn cache_visit_archetype_boolean_preserves_entity_filtering() {
    let a_info = ComponentInfo::of::<A>();
    let archetype = Archetype::new(core::iter::once(&a_info));
    type Dynamic = BorrowOne<Read<A>>;
    type Present = Modified<With<A>>;
    type Absent = Modified<With<B>>;
    macro_rules! check {
        ($ty:ty, $expected:expr) => {
            assert_eq!(
                <$ty>::cache_visit_archetype(&archetype),
                $expected,
                stringify!($ty)
            );
        };
    }
    check!(And<(Dynamic, Present)>, None);
    check!(And<(Present, Dynamic)>, None);
    check!(Or<(Dynamic, Present)>, Some(true));
    check!(Or<(Present, Dynamic)>, Some(true));
    check!(Xor<(Dynamic, Present)>, Some(true));
    check!(Xor<(Present, Dynamic)>, Some(true));
    check!(Xor<(Present, Present)>, Some(true));
    check!(And<(Present, Absent)>, Some(false));
    check!(And<(Absent, Dynamic)>, Some(false));
    check!(And<(Dynamic, Absent)>, Some(false));
    check!(Or<(Dynamic, Absent)>, None);
    check!(Xor<(Absent, Dynamic)>, None);
    check!(And<(Absent, Absent)>, Some(false));
    check!(Or<(Absent, Absent)>, Some(false));
    check!(Xor<(Absent, Absent)>, Some(false));
}

#[test]
fn cache_visit_archetype_filtered_and_requires_absent_arm() {
    let mut world = World::new();
    let before = world.epoch();
    world.spawn((A,));
    world.spawn((A, B));
    let query = And::from_tuple((Modified::<With<A>>::new(before), With::<B>::default()));
    for archetype in world.archetypes() {
        let expected = archetype.has_component(TypeId::of::<A>())
            && archetype.has_component(TypeId::of::<B>());
        assert_eq!(query.visit_archetype(archetype), expected);
        assert_cached(query, archetype);
    }
    assert_eq!(world.view_with(query).iter().count(), 1);
}
