use crate::query::FetchCacheSlot;
use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    Access,
    archetype::{Archetype, ArchetypeComponent},
    component::ComponentInfo,
    epoch::EpochId,
    query::DefaultQuery,
    system::DefaultQueryArg,
    type_id,
};

use super::{AsQuery, BatchFetch, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias};

/// [`Fetch`] type for the `&T` query.
pub struct FetchRead<'a, T> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<T> Send for FetchRead<'_, T> where T: Sync {}

unsafe impl<'a, T> Fetch<'a> for FetchRead<'a, T>
where
    T: 'a,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        FetchRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> &'a T {
        unsafe { &*self.ptr.as_ptr().add(idx as usize) }
    }
}

unsafe impl<'a, T> BatchFetch<'a> for FetchRead<'a, T>
where
    T: 'a,
{
    type Batch = &'a [T];

    #[inline]
    unsafe fn get_batch(&mut self, start: u32, end: u32) -> &'a [T] {
        debug_assert!(end >= start);

        let count = end - start;
        unsafe {
            core::slice::from_raw_parts(self.ptr.as_ptr().add(start as usize), count as usize)
        }
    }
}

marker_type! {
    /// Query for reading component.
    pub struct Read<T>;
}

impl<T> AsQuery for &T
where
    T: 'static,
{
    type Query = Read<T>;
}

impl<T> AsQuery for Read<T>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Read<T>
where
    T: 'static,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for Read<T>
where
    T: 'static,
{
    #[inline]
    fn default_query() -> Self {
        Read
    }
}

impl<T> DefaultQueryArg for Read<T> where T: Sync + 'static {}

unsafe impl<T> Query for Read<T>
where
    T: 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchRead<'a, T>;

    const MUTABLE: bool = false;

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
        let _ = epoch;
        FetchRead {
            ptr: cached.ptr,
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
        unsafe { FetchRead::from_component(component, epoch) }
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
        Some(unsafe { FetchRead::from_component(component, epoch) })
    }

    #[inline]
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        Some(archetype.has_component(type_id::<T>()))
    }

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<T>() {
            Ok(Some(Access::Read))
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
        f(type_id::<T>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchRead<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        unsafe { FetchRead::from_component(component, epoch) }
    }
}

unsafe impl<T> ImmutableQuery for Read<T> where T: 'static {}
unsafe impl<T> SendQuery for Read<T> where T: Sync + 'static {}

impl<'a, T: 'static> FetchRead<'a, T> {
    #[inline]
    unsafe fn from_component(component: &'a ArchetypeComponent, _epoch: EpochId) -> Self {
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data() };

        FetchRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}
