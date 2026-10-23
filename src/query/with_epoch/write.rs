use crate::query::FetchCacheSlot;
use alloc::boxed::Box;
use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{Archetype, ArchetypeComponent},
    component::ComponentInfo,
    epoch::EpochId,
    query::{Access, AsQuery, DefaultQuery, Fetch, IntoQuery, Query, SendQuery, Write, WriteAlias},
    system::DefaultQueryArg,
    type_id,
};

use super::WithEpoch;

/// [`Fetch`] type for the [`WithEpoch<&mut T>`] query.
pub struct WithEpochFetchWrite<'a, T> {
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    archetype_epoch: NonNull<EpochId>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for WithEpochFetchWrite<'a, T>
where
    T: 'a,
{
    type Item = (&'a mut T, EpochId);

    #[inline]
    fn dangling() -> Self {
        WithEpochFetchWrite {
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            epoch: EpochId::start(),
            archetype_epoch: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_epoch.bump(self.epoch);
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> (&'a mut T, EpochId) {
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };

        let prev_epoch = *entity_epoch;
        entity_epoch.bump(self.epoch);

        let item = unsafe { &mut *self.ptr.as_ptr().add(idx as usize) };
        (item, prev_epoch)
    }
}

impl<T> AsQuery for WithEpoch<&mut T>
where
    T: 'static,
{
    type Query = WithEpoch<Write<T>>;
}

impl<T> AsQuery for WithEpoch<Write<T>>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for WithEpoch<Write<T>>
where
    T: 'static,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for WithEpoch<Write<T>>
where
    T: 'static,
{
    #[inline]
    fn default_query() -> Self {
        WithEpoch(Write)
    }
}

impl<T> DefaultQueryArg for WithEpoch<Write<T>> where T: Send + 'static {}

unsafe impl<T> Query for WithEpoch<Write<T>>
where
    T: 'static,
{
    type Item<'a> = (&'a mut T, EpochId);
    type Fetch<'a> = WithEpochFetchWrite<'a, T>;

    const MUTABLE: bool = true;

    const SUPPORTS_FETCH_REUSE: bool = true;
    const FETCH_REUSE_MATCHES_STABLE: bool = true;

    #[inline]
    unsafe fn reuse_fetch<'cached, 'a>(
        &self,
        cached: &mut Self::Fetch<'cached>,
        _arch_idx: u32,
        _archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Self::Fetch<'a> {
        if cached.epoch != epoch {
            unsafe {
                cached.archetype_epoch.as_mut().bump(epoch);
            }
            cached.epoch = epoch;
        }
        WithEpochFetchWrite {
            ptr: cached.ptr,
            entity_epochs: cached.entity_epochs,
            chunk_epochs: cached.chunk_epochs,
            epoch,
            archetype_epoch: cached.archetype_epoch,
            marker: PhantomData,
        }
    }

    const FETCH_CACHE_SLOTS: usize = 1;

    #[inline]
    fn prepare_fetch_cache(archetype: &Archetype, slots: &mut [FetchCacheSlot]) {
        slots[0] = FetchCacheSlot::component(archetype, type_id::<T>());
    }

    #[inline]
    unsafe fn fetch_cached<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
        slots: &[FetchCacheSlot],
    ) -> Self::Fetch<'a> {
        let component = unsafe { slots[0].get(archetype).unwrap_unchecked() };
        unsafe { WithEpochFetchWrite::from_component(component, epoch) }
    }

    #[inline]
    unsafe fn fetch_optional_cached<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
        slots: &[FetchCacheSlot],
    ) -> Option<Self::Fetch<'a>> {
        let component = unsafe { slots[0].get(archetype) }?;
        Some(unsafe { WithEpochFetchWrite::from_component(component, epoch) })
    }
    const FILTERS_ENTITIES: bool = false;

    #[inline]
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        <Write<T> as Query>::cache_visit_archetype(archetype)
    }

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        self.0.component_access(comp)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.0.visit_archetype(archetype)
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        unsafe {
            self.0.access_archetype(archetype, f);
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> WithEpochFetchWrite<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        unsafe { WithEpochFetchWrite::from_component(component, epoch) }
    }
}

unsafe impl<T> SendQuery for WithEpoch<Write<T>> where T: Send + 'static {}

impl<'a, T: 'static> WithEpochFetchWrite<'a, T> {
    #[inline]
    unsafe fn from_component(component: &'a ArchetypeComponent, epoch: EpochId) -> Self {
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data_mut() };
        data.epoch.bump(epoch);

        WithEpochFetchWrite {
            ptr: data.ptr.cast(),
            entity_epochs: unsafe {
                NonNull::new_unchecked(Box::as_mut_ptr(&mut data.entity_epochs).cast::<EpochId>())
            },
            chunk_epochs: unsafe {
                NonNull::new_unchecked(Box::as_mut_ptr(&mut data.chunk_epochs).cast::<EpochId>())
            },
            epoch,
            archetype_epoch: component.epoch_ptr(),
            marker: PhantomData,
        }
    }
}
