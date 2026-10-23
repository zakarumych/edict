use core::any::TypeId;
use edict::{
    Access,
    archetype::Archetype,
    component::{Component, ComponentInfo},
    epoch::EpochId,
    query::{AsQuery, DefaultQuery, Fetch, IntoQuery, Modified, Query, With, WriteAlias},
    world::World,
};

#[derive(Component)]
struct Value(u32);
#[derive(Component)]
struct Other;

#[test]
fn local_indexing_allocates_fresh_epochs_and_shared_mode_resumes_after_bridge() {
    let mut world = World::new();
    let a = world.spawn((Value(0),)).id();
    let b = world.spawn((Value(0),)).id();
    let cutoff = world.epoch();
    {
        let local = world.local();
        let mut writes = local.view::<&mut Value>();
        for index in 0..600 {
            let before = local.epoch();
            writes
                .try_get_mut(if index % 2 == 0 { a } else { b })
                .unwrap()
                .0 += 1;
            assert!(local.epoch().after(before));
        }
        drop(writes);
        assert_eq!(
            local
                .view_with(Modified::<With<Value>>::new(cutoff))
                .iter()
                .count(),
            2
        );
        let local_end = local.epoch();
        {
            let mut shared_view = local.as_world().view::<&mut Value>();
            shared_view.try_get_mut(a).unwrap().0 += 1;
        }
        assert!(local.epoch().after(local_end));
        let shared_end = local.epoch();
        local.view::<&mut Value>().try_get_mut(b).unwrap().0 += 1;
        assert!(local.epoch().after(shared_end));
    }
    assert_eq!(world.get::<&Value>(a).unwrap().0, 301);
    assert_eq!(world.get::<&Value>(b).unwrap().0, 301);
    let after_local = world.epoch();
    world.get::<&mut Value>(a).unwrap().0 += 1;
    assert!(world.epoch().after(after_local));
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
        panic!("Fresh indexed fetch required")
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

#[test]
fn custom_indexed_fetches_receive_fresh_epochs_and_complete_protocol() {
    let mut world = World::new();
    let id = world.spawn((Other,)).id();
    let local = world.local();
    let mut previous = local.epoch();
    let mut view = local.view::<Strict>();
    for _ in 0..600 {
        let epoch = view.try_get_mut(id).unwrap();
        assert!(epoch.after(previous));
        assert_eq!(local.epoch(), epoch);
        previous = epoch;
    }
}

#[test]
fn readonly_and_late_rejected_accesses_leave_epoch_unchanged_but_mutable_filter_allocates() {
    let mut world = World::new();
    let id = world.spawn((Value(0), Other)).id();
    let local = world.local();
    let before = local.epoch();
    assert_eq!(local.view::<&Value>().try_get(id).unwrap().0, 0);
    assert_eq!(local.epoch(), before);
    assert!(
        local
            .view_with(Modified::<edict::query::Write<Value>>::new(before))
            .try_get_mut(id)
            .is_err()
    );
    assert_eq!(local.epoch(), before);
    let mut view = local.view_filter::<&Other, Strict>();
    assert!(view.try_get_mut(id).is_ok());
    assert!(local.epoch().after(before));
}
