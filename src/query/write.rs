use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype, component::ComponentInfo, epoch::EpochId, system::QueryArg, type_id,
};

use super::{
    Access, AsQuery, BatchFetch, DefaultQuery, Fetch, IntoQuery, Query, SendQuery, WriteAlias,
};

/// [`Fetch`] type for the `&mut T` query.
pub struct FetchWrite<'a, T> {
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<T> Send for FetchWrite<'_, T> where T: Send {}

unsafe impl<'a, T> Fetch<'a> for FetchWrite<'a, T>
where
    T: 'a,
{
    type Item = &'a mut T;

    #[inline(always)]
    fn dangling() -> Self {
        FetchWrite {
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_epoch.bump(self.epoch);
    }

    #[inline(always)]
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

    #[inline(always)]
    unsafe fn get_batch(&mut self, start: u32, end: u32) -> &'a mut [T] {
        debug_assert!(end >= start);

        let count = end - start;
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

impl<T> DefaultQuery for &mut T
where
    T: 'static,
{
    #[inline(always)]
    fn default_query() -> Write<T> {
        Write
    }
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
    #[inline(always)]
    fn into_query(self) -> Write<T> {
        Write
    }
}

impl<T> DefaultQuery for Write<T>
where
    T: 'static,
{
    #[inline(always)]
    fn default_query() -> Write<T> {
        Write
    }
}

impl<T> QueryArg for Write<T>
where
    T: Send + 'static,
{
    #[inline(always)]
    fn new() -> Write<T> {
        Write
    }
}

unsafe impl<T> Query for Write<T>
where
    T: 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchWrite<'a, T>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<T>() {
            Ok(Some(Access::Write))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(type_id::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(type_id::<T>(), Access::Write)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchWrite<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data_mut() };
        data.epoch.bump(epoch);

        FetchWrite {
            ptr: data.ptr.cast(),
            entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
            chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
            epoch,
            marker: PhantomData,
        }
    }
}

unsafe impl<T> SendQuery for Write<T> where T: Send + 'static {}
