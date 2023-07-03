use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{write::Write, Access, Fetch, IntoQuery, Query},
};

use super::Modified;

/// [`Fetch`] type for the [`Modified<&mut T>`] query.
pub struct ModifiedFetchWrite<'a, T> {
    after_epoch: EpochId,
    epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchWrite<'a, T>
where
    T: Send + 'a,
{
    type Item = &'a mut T;

    #[inline(always)]
    fn dangling() -> Self {
        ModifiedFetchWrite {
            after_epoch: EpochId::start(),
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        let chunk_epoch = *self.chunk_epochs.as_ptr().add(chunk_idx as usize);
        chunk_epoch.after(self.after_epoch)
    }

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx as usize);
        epoch.after(self.after_epoch)
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

impl<T> IntoQuery for Modified<&mut T>
where
    T: Send + 'static,
{
    type Query = Modified<Write<T>>;

    fn into_query(self) -> Modified<Write<T>> {
        Modified {
            after_epoch: self.after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> IntoQuery for Modified<Write<T>>
where
    T: Send + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self {
        self
    }
}

unsafe impl<T> Query for Modified<Write<T>>
where
    T: Send + 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = ModifiedFetchWrite<'a, T>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        Write::<T>.access(ty)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => false,
            Some(component) => unsafe {
                debug_assert_eq!(Write::<T>.visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data_mut();
                data.epoch.after(self.after_epoch)
            },
        }
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
    ) -> ModifiedFetchWrite<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        let data = component.data_mut();

        debug_assert!(data.epoch.after(self.after_epoch));
        data.epoch.bump(epoch);

        ModifiedFetchWrite {
            after_epoch: self.after_epoch,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            marker: PhantomData,
        }
    }
}

impl<T> IntoQuery for Modified<Option<&mut T>>
where
    T: Send + 'static,
{
    type Query = Modified<Option<Write<T>>>;

    fn into_query(self) -> Modified<Option<Write<T>>> {
        Modified {
            after_epoch: self.after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> IntoQuery for Modified<Option<Write<T>>>
where
    T: Send + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self {
        self
    }
}

unsafe impl<T> Query for Modified<Option<Write<T>>>
where
    T: Send + 'static,
{
    type Item<'a> = Option<&'a mut T>;
    type Fetch<'a> = Option<ModifiedFetchWrite<'a, T>>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        Write::<T>.access(ty)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(Write::<T>.visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data();
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if let Some(component) = archetype.component(TypeId::of::<T>()) {
            debug_assert_eq!(Write::<T>.visit_archetype(archetype), true);

            debug_assert_eq!(component.id(), TypeId::of::<T>());
            let data = component.data();
            if data.epoch.after(self.after_epoch) {
                f(TypeId::of::<T>(), Access::Read)
            }
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Option<ModifiedFetchWrite<'a, T>> {
        match archetype.component(TypeId::of::<T>()) {
            None => None,
            Some(component) => {
                let data = component.data();

                debug_assert!(data.epoch.after(self.after_epoch));

                Some(ModifiedFetchWrite {
                    after_epoch: self.after_epoch,
                    epoch,
                    ptr: data.ptr.cast(),
                    entity_epochs: NonNull::new_unchecked(
                        data.entity_epochs.as_ptr() as *mut EpochId
                    ),
                    chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_ptr() as *mut EpochId),
                    marker: PhantomData,
                })
            }
        }
    }
}
