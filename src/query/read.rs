use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype, component::ComponentInfo, epoch::EpochId, system::QueryArg, type_id,
    Access,
};

use super::{
    AsQuery, BatchFetch, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery,
    WriteAlias,
};

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

impl<T> DefaultQuery for &T
where
    T: 'static,
{
    #[inline]
    fn default_query() -> Read<T> {
        Read
    }
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
    fn default_query() -> Read<T> {
        Read
    }
}

impl<T> QueryArg for Read<T>
where
    T: Sync + 'static,
{
    #[inline]
    fn new() -> Read<T> {
        Read
    }
}

unsafe impl<T> Query for Read<T>
where
    T: 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchRead<'a, T>;

    const MUTABLE: bool = false;

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
        _epoch: EpochId,
    ) -> FetchRead<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data() };

        FetchRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Read<T> where T: 'static {}
unsafe impl<T> SendQuery for Read<T> where T: Sync + 'static {}
