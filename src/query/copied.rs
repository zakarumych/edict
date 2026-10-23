use crate::query::FetchCacheSlot;
use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{Archetype, ArchetypeComponent},
    component::ComponentInfo,
    epoch::EpochId,
    query::DefaultQuery,
    system::DefaultQueryArg,
    type_id,
};

use super::{Access, AsQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias};

/// [`Fetch`] type for the `&T` query.
pub struct FetchCpy<'a, T> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchCpy<'a, T>
where
    T: Copy + 'a,
{
    type Item = T;

    #[inline]
    fn dangling() -> Self {
        FetchCpy {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> T {
        unsafe { *self.ptr.as_ptr().add(idx as usize) }
    }
}

marker_type! {
    /// Query for fetching a copy of a component.
    /// Borrows component immutably and yields a copy.
    /// Prefer this over `&T` or [`Read<T>`] for small `Copy` types.
    ///
    /// [`Read<T>`]: crate::query::Read
    pub struct Cpy<T>;
}

impl<T> AsQuery for Cpy<T>
where
    T: Copy + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Cpy<T>
where
    T: Copy + 'static,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for Cpy<T>
where
    T: Copy + 'static,
{
    #[inline]
    fn default_query() -> Self {
        Cpy
    }
}

impl<T> DefaultQueryArg for Cpy<T> where T: Copy + Sync + 'static {}

unsafe impl<T> Query for Cpy<T>
where
    T: Copy + 'static,
{
    type Item<'a> = T;
    type Fetch<'a> = FetchCpy<'a, T>;

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
        FetchCpy {
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
        unsafe { FetchCpy::from_component(component, epoch) }
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
        Some(unsafe { FetchCpy::from_component(component, epoch) })
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
    ) -> FetchCpy<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        unsafe { FetchCpy::from_component(component, epoch) }
    }
}

unsafe impl<T> ImmutableQuery for Cpy<T> where T: Copy + 'static {}

unsafe impl<T> SendQuery for Cpy<T> where T: Sync + Copy + 'static {}

impl<'a, T: Copy + 'static> FetchCpy<'a, T> {
    #[inline]
    unsafe fn from_component(component: &'a ArchetypeComponent, _epoch: EpochId) -> Self {
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data() };

        FetchCpy {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}
