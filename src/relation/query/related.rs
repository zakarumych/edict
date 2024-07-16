use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    entity::EntityId,
    epoch::EpochId,
    query::{
        AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, Read, SendQuery, With,
        Write, WriteAlias,
    },
    relation::{OriginComponent, Relation, TargetComponent},
    system::QueryArg,
    type_id, Access,
};

use super::{RelationIter, RelationReadIter, RelationWriteIter};

marker_type! {
    /// Query for target of relation.
    ///
    /// Yields slices of origin ids for each target.
    pub struct Related<R>;
}

impl<R> AsQuery for Related<With<R>>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for Related<With<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for Related<With<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        Related
    }
}

/// Fetch type for [`Related<With<R>>`]
pub struct FetchRelatedWith<'a, R> {
    ptr: NonNull<u8>,
    marker: PhantomData<&'a (EntityId, R)>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatedWith<'a, R>
where
    R: Relation,
{
    type Item = RelationIter<'a, R>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatedWith {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> RelationIter<'a, R> {
        if R::SYMMETRIC {
            let component = unsafe {
                &*self
                    .ptr
                    .cast::<OriginComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };

            RelationIter::new(component.targets())
        } else {
            let component = unsafe {
                &*self
                    .ptr
                    .cast::<TargetComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };

            RelationIter::new(component.origins())
        }
    }
}

unsafe impl<R> Query for Related<With<R>>
where
    R: Relation,
{
    type Item<'a> = RelationIter<'a, R>;
    type Fetch<'a> = FetchRelatedWith<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if R::SYMMETRIC {
            if comp.id() == type_id::<OriginComponent<R>>() {
                Ok(Some(Access::Read))
            } else {
                Ok(None)
            }
        } else {
            if comp.id() == type_id::<TargetComponent<R>>() {
                Ok(Some(Access::Read))
            } else {
                Ok(None)
            }
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        if R::SYMMETRIC {
            archetype.has_component(type_id::<OriginComponent<R>>())
        } else {
            archetype.has_component(type_id::<TargetComponent<R>>())
        }
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if R::SYMMETRIC {
            f(type_id::<OriginComponent<R>>(), Access::Read)
        } else {
            f(type_id::<TargetComponent<R>>(), Access::Read)
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelatedWith<'a, R> {
        if R::SYMMETRIC {
            let component = unsafe {
                archetype
                    .component(type_id::<OriginComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

            let data = unsafe { component.data() };

            FetchRelatedWith {
                ptr: data.ptr.cast(),
                marker: PhantomData,
            }
        } else {
            let component = unsafe {
                archetype
                    .component(type_id::<TargetComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), type_id::<TargetComponent<R>>());

            let data = unsafe { component.data() };

            FetchRelatedWith {
                ptr: data.ptr.cast(),
                marker: PhantomData,
            }
        }
    }
}

unsafe impl<R> ImmutableQuery for Related<With<R>> where R: Relation {}
unsafe impl<R> SendQuery for Related<With<R>> where R: Relation {}

impl<R> QueryArg for Related<With<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn new() -> Self {
        Related
    }
}

impl<R> AsQuery for Related<&R>
where
    R: Relation,
{
    type Query = Related<Read<R>>;
}

impl<R> DefaultQuery for Related<&R>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Related<Read<R>> {
        Related
    }
}

impl<R> AsQuery for Related<Read<R>>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for Related<Read<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for Related<Read<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        Related
    }
}

/// Fetch type for [`Related<R>`]
pub struct FetchRelatedRead<'a, R> {
    ptr: NonNull<u8>,
    marker: PhantomData<&'a (EntityId, R)>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatedRead<'a, R>
where
    R: Relation,
{
    type Item = RelationReadIter<'a, R>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatedRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> RelationReadIter<'a, R> {
        if R::SYMMETRIC {
            let component = unsafe {
                &*self
                    .ptr
                    .cast::<OriginComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };

            RelationReadIter::new(component.targets())
        } else {
            let component = unsafe {
                &*self
                    .ptr
                    .cast::<TargetComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };

            RelationReadIter::new(component.origins())
        }
    }
}

unsafe impl<R> Query for Related<Read<R>>
where
    R: Relation,
{
    type Item<'a> = RelationReadIter<'a, R>;
    type Fetch<'a> = FetchRelatedRead<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<TargetComponent<R>>() {
            Ok(Some(Access::Read))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        if R::SYMMETRIC {
            archetype.has_component(type_id::<OriginComponent<R>>())
        } else {
            archetype.has_component(type_id::<TargetComponent<R>>())
        }
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if R::SYMMETRIC {
            f(type_id::<OriginComponent<R>>(), Access::Read)
        } else {
            f(type_id::<TargetComponent<R>>(), Access::Read)
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelatedRead<'a, R> {
        if R::SYMMETRIC {
            let component = unsafe {
                archetype
                    .component(type_id::<OriginComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

            let data = unsafe { component.data() };

            FetchRelatedRead {
                ptr: data.ptr.cast(),
                marker: PhantomData,
            }
        } else {
            let component = unsafe {
                archetype
                    .component(type_id::<TargetComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), type_id::<TargetComponent<R>>());

            let data = unsafe { component.data() };

            FetchRelatedRead {
                ptr: data.ptr.cast(),
                marker: PhantomData,
            }
        }
    }
}

unsafe impl<R> ImmutableQuery for Related<Read<R>> where R: Relation {}
unsafe impl<R> SendQuery for Related<Read<R>> where R: Relation + Sync {}

impl<R> QueryArg for Related<Read<R>>
where
    R: Relation + Sync,
{
    #[inline(always)]
    fn new() -> Self {
        Related
    }
}

impl<R> AsQuery for Related<&mut R>
where
    R: Relation,
{
    type Query = Related<Write<R>>;
}

impl<R> DefaultQuery for Related<&mut R>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Related<Write<R>> {
        Related
    }
}

impl<R> AsQuery for Related<Write<R>>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for Related<Write<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for Related<Write<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        Related
    }
}

/// Fetch type for [`Related<R>`]
pub struct FetchRelatedWrite<'a, R> {
    ptr: NonNull<u8>,
    epoch: EpochId,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a (EntityId, R)>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatedWrite<'a, R>
where
    R: Relation,
{
    type Item = RelationWriteIter<'a, R>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatedWrite {
            ptr: NonNull::dangling(),
            epoch: EpochId::start(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_epoch.bump(self.epoch);
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> RelationWriteIter<'a, R> {
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
        entity_epoch.bump(self.epoch);

        if R::SYMMETRIC {
            let component = unsafe {
                &mut *self
                    .ptr
                    .cast::<OriginComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };

            RelationWriteIter::new(component.targets_mut())
        } else {
            let component = unsafe {
                &mut *self
                    .ptr
                    .cast::<TargetComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };

            RelationWriteIter::new(component.origins_mut())
        }
    }
}

unsafe impl<R> Query for Related<Write<R>>
where
    R: Relation,
{
    type Item<'a> = RelationWriteIter<'a, R>;
    type Fetch<'a> = FetchRelatedWrite<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<TargetComponent<R>>() {
            Ok(Some(Access::Write))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        if R::SYMMETRIC {
            archetype.has_component(type_id::<OriginComponent<R>>())
        } else {
            archetype.has_component(type_id::<TargetComponent<R>>())
        }
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if R::SYMMETRIC {
            f(type_id::<OriginComponent<R>>(), Access::Write)
        } else {
            f(type_id::<TargetComponent<R>>(), Access::Write)
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchRelatedWrite<'a, R> {
        if R::SYMMETRIC {
            let component = unsafe {
                archetype
                    .component(type_id::<TargetComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), type_id::<TargetComponent<R>>());

            let data = unsafe { component.data_mut() };
            data.epoch.bump(epoch);

            FetchRelatedWrite {
                ptr: data.ptr.cast(),
                epoch,
                entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
                chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
                marker: PhantomData,
            }
        } else {
            let component = unsafe {
                archetype
                    .component(type_id::<OriginComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

            let data = unsafe { component.data_mut() };
            data.epoch.bump(epoch);

            FetchRelatedWrite {
                ptr: data.ptr.cast(),
                epoch,
                entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
                chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
                marker: PhantomData,
            }
        }
    }
}

unsafe impl<R> ImmutableQuery for Related<Write<R>> where R: Relation {}
unsafe impl<R> SendQuery for Related<Write<R>> where R: Relation + Send {}

impl<R> QueryArg for Related<Write<R>>
where
    R: Relation + Send,
{
    #[inline(always)]
    fn new() -> Self {
        Related
    }
}
