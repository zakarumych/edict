use crate::query::FetchCacheSlot;
use alloc::boxed::Box;
use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{Archetype, ArchetypeComponent},
    component::ComponentInfo,
    epoch::EpochId,
    query::DefaultQuery,
    system::DefaultQueryArg,
    type_id,
};

use super::{Access, AsQuery, BatchFetch, Fetch, IntoQuery, Query, SendQuery, WriteAlias};

/// [`Fetch`] type for the `&mut T` query.
pub struct FetchWrite<'a, T> {
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    archetype_epoch: NonNull<EpochId>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<T> Send for FetchWrite<'_, T> where T: Send {}

unsafe impl<'a, T> Fetch<'a> for FetchWrite<'a, T>
where
    T: 'a,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchWrite {
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
    unsafe fn get_item(&mut self, idx: u32) -> &'a mut T {
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
        entity_epoch.bump(self.epoch);

        unsafe { &mut *self.ptr.as_ptr().add(idx as usize) }
    }
}

unsafe impl<'a, T> BatchFetch<'a> for FetchWrite<'a, T>
where
    T: 'a,
{
    type Batch = &'a mut [T];

    #[inline]
    unsafe fn get_batch(&mut self, start: u32, end: u32) -> &'a mut [T] {
        debug_assert!(end >= start);

        let count = end - start;
        for idx in start..end {
            let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
            entity_epoch.bump(self.epoch);
        }
        unsafe {
            core::slice::from_raw_parts_mut(self.ptr.as_ptr().add(start as usize), count as usize)
        }
    }
}

marker_type! {
    /// Query for writing a component.
    pub struct Write<T>;
}

impl<T> AsQuery for &mut T
where
    T: 'static,
{
    type Query = Write<T>;
}

impl<T> AsQuery for Write<T>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Write<T>
where
    T: 'static,
{
    #[inline]
    fn into_query(self) -> Write<T> {
        Write
    }
}

impl<T> DefaultQuery for Write<T>
where
    T: 'static,
{
    #[inline]
    fn default_query() -> Self {
        Write
    }
}

impl<T> DefaultQueryArg for Write<T> where T: Send + 'static {}

unsafe impl<T> Query for Write<T>
where
    T: 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchWrite<'a, T>;

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
        FetchWrite {
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
        unsafe { FetchWrite::from_component(component, epoch) }
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
        Some(unsafe { FetchWrite::from_component(component, epoch) })
    }

    #[inline]
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        Some(archetype.has_component(type_id::<T>()))
    }

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<T>() {
            Ok(Some(Access::Write))
        } else {
            Ok(None)
        }
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(type_id::<T>())
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(type_id::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchWrite<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        unsafe { FetchWrite::from_component(component, epoch) }
    }
}

unsafe impl<T> SendQuery for Write<T> where T: Send + 'static {}

impl<'a, T: 'static> FetchWrite<'a, T> {
    #[inline]
    unsafe fn from_component(component: &'a ArchetypeComponent, epoch: EpochId) -> Self {
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data_mut() };
        data.epoch.bump(epoch);

        FetchWrite {
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
