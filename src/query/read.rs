use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, epoch::EpochId, system::QueryArg, Access};

use super::{DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, WriteAlias};

/// [`Fetch`] type for the `&T` query.

pub struct FetchRead<'a, T> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchRead<'a, T>
where
    T: Sync + 'a,
{
    type Item = &'a T;

    #[inline(always)]
    fn dangling() -> Self {
        FetchRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> &'a T {
        &*self.ptr.as_ptr().add(idx as usize)
    }
}

marker_type! {
    /// Query for reading component.
    pub struct Read<T>;
}

impl<T> IntoQuery for &T
where
    T: Sync + 'static,
{
    type Query = Read<T>;

    #[inline(always)]
    fn into_query(self) -> Read<T> {
        Read
    }
}

impl<T> DefaultQuery for &T
where
    T: Sync + 'static,
{
    #[inline(always)]
    fn default_query() -> Read<T> {
        Read
    }
}

impl<T> IntoQuery for Read<T>
where
    T: Sync + 'static,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for Read<T>
where
    T: Sync + 'static,
{
    #[inline(always)]
    fn default_query() -> Read<T> {
        Read
    }
}

impl<T> QueryArg for Read<T>
where
    T: Sync + 'static,
{
    #[inline(always)]
    fn new() -> Read<T> {
        Read
    }
}

unsafe impl<T> Query for Read<T>
where
    T: Sync + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchRead<'a, T>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_type_access(&self, ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(Access::read_type::<T>(ty))
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRead<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data();

        FetchRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Read<T> where T: Sync + 'static {}
