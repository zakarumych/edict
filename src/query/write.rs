use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, epoch::EpochId, system::QueryArg};

use super::{Access, AsQuery, DefaultQuery, Fetch, IntoQuery, Query, SendQuery, WriteAlias};

/// [`Fetch`] type for the `&mut T` query.
pub struct FetchWrite<'a, T> {
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    marker: PhantomData<&'a mut [T]>,
}

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
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize);
        chunk_epoch.bump(self.epoch);
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> &'a mut T {
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx as usize);
        entity_epoch.bump(self.epoch);

        &mut *self.ptr.as_ptr().add(idx as usize)
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
    fn component_type_access(&self, ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(Access::write_type::<T>(ty))
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Write)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchWrite<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data_mut();
        data.epoch.bump(epoch);

        FetchWrite {
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            epoch,
            marker: PhantomData,
        }
    }
}

unsafe impl<T> SendQuery for Write<T> where T: Send + 'static {}
