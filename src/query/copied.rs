use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype, component::ComponentInfo, epoch::EpochId, system::QueryArg, type_id,
};

use super::{
    Access, AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias,
};

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

    #[inline(always)]
    fn dangling() -> Self {
        FetchCpy {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
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
    type Fetch<'a> = FetchCpy<'a, T>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<T>() {
            Ok(Some(Access::Read))
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
        f(type_id::<T>(), Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchCpy<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data() };

        FetchCpy {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Cpy<T> where T: Copy + 'static {}

unsafe impl<T> SendQuery for Cpy<T> where T: Sync + Copy + 'static {}
