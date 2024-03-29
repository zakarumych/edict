use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, epoch::EpochId, system::QueryArg};

use super::{
    Access, AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias,
};

/// [`Fetch`] type for the `&T` query.

pub struct FetchCopied<'a, T> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchCopied<'a, T>
where
    T: Copy + 'a,
{
    type Item = T;

    #[inline(always)]
    fn dangling() -> Self {
        FetchCopied {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> T {
        *self.ptr.as_ptr().add(idx as usize)
    }
}

marker_type! {
    /// Query for fetching a copy of a component.
    /// Borrows component immutably and yields a copy.
    /// Prefer this over [`&T`] or [`Read<T>`] for small `Copy` types.
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
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for Cpy<T>
where
    T: Copy + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        Cpy
    }
}

impl<T> QueryArg for Cpy<T>
where
    T: Copy + Sync + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        Cpy
    }
}

unsafe impl<T> Query for Cpy<T>
where
    T: Copy + 'static,
{
    type Item<'a> = T;
    type Fetch<'a> = FetchCopied<'a, T>;

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
    ) -> FetchCopied<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data();

        FetchCopied {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Cpy<T> where T: Copy + 'static {}

unsafe impl<T> SendQuery for Cpy<T> where T: Sync + Copy + 'static {}
