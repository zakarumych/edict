use std::sync::atomic::{AtomicU64, Ordering};

use edict::{
    Access, EntityError,
    archetype::Archetype,
    component::{Component, ComponentInfo},
    entity::{Entity, EntityId, EntityLoc, EntityRef, EntitySet, Location},
    epoch::EpochId,
    query::{AsQuery, Entities, EpochOf, Fetch, IntoQuery, Modified, Query, Read, WriteAlias},
    world::World,
};

#[derive(Component)]
struct A(i32);
#[derive(Component)]
struct B;

#[test]
fn exclusive_and_shared_access_preserve_epochs_and_errors() {
    let mut exclusive = World::new();
    let mut shared = World::new();
    let e = exclusive.spawn_one(A(7)).id();
    let s = shared.spawn_one(A(7)).id();
    for cached in [false, true] {
        if cached {
            exclusive.maintain_query_cache::<<&A as AsQuery>::Query>();
            shared.maintain_query_cache::<<&A as AsQuery>::Query>();
        }
        let before = exclusive.epoch();
        assert_eq!(exclusive.get::<&A>(e).unwrap().0, 7);
        assert_eq!(unsafe { shared.get_unchecked::<&A>(s) }.unwrap().0, 7);
        assert!(exclusive.epoch().after(before));
        assert_eq!(exclusive.epoch(), shared.epoch());

        let before = exclusive.epoch();
        assert!(matches!(exclusive.get::<&B>(e), Err(EntityError::Mismatch)));
        assert!(matches!(
            unsafe { shared.get_unchecked::<&B>(s) },
            Err(EntityError::Mismatch)
        ));
        assert_eq!(exclusive.epoch(), before);
        assert_eq!(exclusive.epoch(), shared.epoch());

        exclusive.get::<&mut A>(e).unwrap().0 = 7;
        unsafe { shared.get_unchecked::<&mut A>(s) }.unwrap().0 = 7;
        assert_eq!(exclusive.epoch(), shared.epoch());
        let changed = exclusive.get::<EpochOf<A>>(e).unwrap();
        assert_eq!(
            changed,
            unsafe { shared.get_unchecked::<EpochOf<A>>(s) }.unwrap()
        );
        assert_eq!(exclusive.epoch(), shared.epoch());
        assert!(changed.after(before));
        assert!(
            exclusive
                .get_with(e, Modified::<Read<A>>::new(changed))
                .is_err()
        );
        assert!(
            unsafe { shared.get_with_unchecked(s, Modified::<Read<A>>::new(changed)) }.is_err()
        );
        assert_eq!(exclusive.epoch(), shared.epoch());
    }

    let e = exclusive.allocate().id();
    let s = shared.allocate().id();
    let before = exclusive.epoch();
    assert_eq!(exclusive.get::<Entities>(e).unwrap().id(), e);
    assert_eq!(
        unsafe { shared.get_unchecked::<Entities>(s) }.unwrap().id(),
        s
    );
    assert!(matches!(exclusive.get::<&A>(e), Err(EntityError::Mismatch)));
    assert!(matches!(
        unsafe { shared.get_unchecked::<&A>(s) },
        Err(EntityError::Mismatch)
    ));
    assert_eq!(exclusive.epoch(), before);
    assert_eq!(exclusive.epoch(), shared.epoch());
    exclusive.despawn(e).unwrap();
    shared.despawn(s).unwrap();
    let before = exclusive.epoch();
    assert!(matches!(
        exclusive.get::<&A>(e),
        Err(EntityError::NoSuchEntity)
    ));
    assert!(matches!(
        unsafe { shared.get_unchecked::<&A>(s) },
        Err(EntityError::NoSuchEntity)
    ));
    assert_eq!(exclusive.epoch(), before);
    assert_eq!(exclusive.epoch(), shared.epoch());
}

static EVENTS: AtomicU64 = AtomicU64::new(0);
fn event(value: u64) {
    EVENTS
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(old * 10 + value)
        })
        .unwrap();
}

#[derive(Clone, Copy)]
struct Probe {
    archetype: bool,
    late: bool,
    chunk: bool,
    item: bool,
}
impl AsQuery for Probe {
    type Query = Self;
}
impl IntoQuery for Probe {
    fn into_query(self) -> Self {
        self
    }
}
struct ProbeFetch {
    epoch: EpochId,
    chunk: bool,
    item: bool,
}
unsafe impl Fetch<'_> for ProbeFetch {
    type Item = EpochId;
    fn dangling() -> Self {
        panic!("dangling probe must not be fetched")
    }
    unsafe fn visit_chunk(&mut self, _: u32) -> bool {
        event(3);
        self.chunk
    }
    unsafe fn touch_chunk(&mut self, _: u32) {
        event(4);
    }
    unsafe fn visit_item(&mut self, _: u32) -> bool {
        event(5);
        self.item
    }
    unsafe fn get_item(&mut self, _: u32) -> EpochId {
        event(6);
        self.epoch
    }
}
unsafe impl Query for Probe {
    type Item<'a> = EpochId;
    type Fetch<'a> = ProbeFetch;
    const MUTABLE: bool = false;
    const FILTERS_ENTITIES: bool = true;
    fn cache_visit_archetype(_: &Archetype) -> Option<bool> {
        None
    }
    fn component_access(&self, _: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }
    fn visit_archetype(&self, _: &Archetype) -> bool {
        event(1);
        self.archetype
    }
    unsafe fn visit_archetype_late(&self, _: &Archetype) -> bool {
        event(7);
        assert!(self.archetype);
        self.late
    }
    unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(std::any::TypeId, Access)) {}
    unsafe fn fetch(&self, _: u32, _: &Archetype, epoch: EpochId) -> ProbeFetch {
        assert!(self.archetype && self.late);
        event(2);
        ProbeFetch {
            epoch,
            chunk: self.chunk,
            item: self.item,
        }
    }
}

#[test]
fn exclusive_and_shared_fetch_protocol_and_lazy_epoch_match() {
    for shared in [false, true] {
        for (archetype, late, chunk, item, events) in [
            (false, true, true, true, 1),
            (true, false, true, true, 17),
            (true, true, false, true, 1723),
            (true, true, true, false, 172345),
            (true, true, true, true, 1723456),
        ] {
            let mut world = World::new();
            let entity = world.spawn_one(A(1)).id();
            EVENTS.store(0, Ordering::Relaxed);
            let before = world.epoch();
            let probe = Probe {
                archetype,
                late,
                chunk,
                item,
            };
            let result = if shared {
                unsafe { world.get_with_unchecked(entity, probe) }
            } else {
                world.get_with(entity, probe)
            };
            assert_eq!(EVENTS.load(Ordering::Relaxed), events);
            assert_eq!(world.epoch().after(before), archetype && late);
            match result {
                Ok(epoch) => {
                    assert!(archetype && late && chunk && item);
                    assert_eq!(epoch, world.epoch());
                }
                Err(EntityError::Mismatch) => assert!(!(archetype && late && chunk && item)),
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        }
    }
}

#[derive(Clone, Copy)]
struct FakeEntity {
    id: EntityId,
    location: Location,
}
impl Entity for FakeEntity {
    fn id(&self) -> EntityId {
        self.id
    }
    fn lookup(&self, _: &EntitySet) -> Option<Location> {
        Some(self.location)
    }
    fn is_alive(&self, entities: &EntitySet) -> bool {
        self.id.is_alive(entities)
    }
    fn entity_loc<'a>(&self, entities: &'a EntitySet) -> Option<EntityLoc<'a>> {
        self.id.entity_loc(entities)
    }
    fn entity_ref<'a>(&self, world: &'a mut World) -> Option<EntityRef<'a>> {
        self.id.entity_ref(world)
    }
}

#[test]
fn fabricated_out_of_bounds_rows_panic_before_fetch_in_all_builds() {
    let mut world = World::new();
    let id = world.spawn_one(A(1)).id();
    let arch = world
        .archetypes()
        .iter()
        .position(|archetype| archetype.has_component(std::any::TypeId::of::<A>()))
        .unwrap() as u32;
    for row in [1, 2, u32::MAX] {
        let fake = FakeEntity {
            id,
            location: Location { arch, idx: row },
        };
        let before = world.epoch();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = world.get::<&A>(fake);
            }))
            .is_err()
        );
        assert_eq!(world.epoch(), before);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                let _ = world.get_unchecked::<&A>(fake);
            }))
            .is_err()
        );
        assert_eq!(world.epoch(), before);
    }
}
