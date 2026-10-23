use alloc::boxed::Box;
use core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{Archetype, chunk_idx},
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        Access, Alt, AsQuery, DefaultQuery, Fetch, IntoQuery, Query, RefMut, SendQuery, WriteAlias,
    },
    system::DefaultQueryArg,
    type_id,
};

use super::WithEpoch;

/// [`Fetch`] type for the [`WithEpoch<Alt<T>>`] query.
pub struct WithEpochFetchAlt<'a, T> {
    epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<Cell<EpochId>>,
    archetype_epoch: NonNull<Cell<EpochId>>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for WithEpochFetchAlt<'a, T>
where
    T: 'a,
{
    type Item = (RefMut<'a, T>, EpochId);

    #[inline]
    fn dangling() -> Self {
        WithEpochFetchAlt {
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            archetype_epoch: NonNull::dangling(),
            epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        debug_assert!((*chunk_epoch).get().before(self.epoch));
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> (RefMut<'a, T>, EpochId) {
        let archetype_epoch = unsafe { &mut *self.archetype_epoch.as_ptr() };
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx(idx) as usize) };
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };

        debug_assert!(entity_epoch.before(self.epoch));
        let prev_epoch = *entity_epoch;

        let ref_mut = RefMut {
            component: unsafe { &mut *self.ptr.as_ptr().add(idx as usize) },
            entity_epoch,
            chunk_epoch,
            archetype_epoch,
            epoch: self.epoch,
        };

        (ref_mut, prev_epoch)
    }
}

impl<T> AsQuery for WithEpoch<Alt<T>>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for WithEpoch<Alt<T>>
where
    T: 'static,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for WithEpoch<Alt<T>>
where
    T: 'static,
{
    #[inline]
    fn default_query() -> Self {
        WithEpoch(Alt)
    }
}

impl<T> DefaultQueryArg for WithEpoch<Alt<T>> where T: Send + 'static {}

unsafe impl<T> Query for WithEpoch<Alt<T>>
where
    T: 'static,
{
    type Item<'a> = (RefMut<'a, T>, EpochId);
    type Fetch<'a> = WithEpochFetchAlt<'a, T>;

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
        cached.epoch = epoch;
        WithEpochFetchAlt {
            ptr: cached.ptr,
            entity_epochs: cached.entity_epochs,
            chunk_epochs: cached.chunk_epochs,
            epoch,
            archetype_epoch: cached.archetype_epoch,
            marker: PhantomData,
        }
    }
    const FILTERS_ENTITIES: bool = false;

    #[inline]
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        <Alt<T> as Query>::cache_visit_archetype(archetype)
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
    ) -> WithEpochFetchAlt<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data_mut() };

        WithEpochFetchAlt {
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: unsafe {
                NonNull::new_unchecked(Box::as_mut_ptr(&mut data.entity_epochs).cast::<EpochId>())
            },
            chunk_epochs: unsafe {
                NonNull::new_unchecked(Box::as_mut_ptr(&mut data.chunk_epochs).cast::<EpochId>())
            }
            .cast(),
            archetype_epoch: component.epoch_ptr().cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> SendQuery for WithEpoch<Alt<T>> where T: Send + 'static {}
