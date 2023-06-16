use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{phantom::PhantomQuery, Access, Fetch, ImmutableQuery, IntoQuery, Query},
};

use super::Modified;

/// [`Fetch`] type for the [`Modified<&T>`] query.
pub struct ModifiedFetchRead<'a, T> {
    after_epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchRead<'a, T>
where
    T: Sync + 'a,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchRead {
            after_epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        let chunk_epoch = *self.chunk_epochs.as_ptr().add(chunk_idx as usize);
        chunk_epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx as usize);
        epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> &'a T {
        &*self.ptr.as_ptr().add(idx as usize)
    }
}

impl<T> IntoQuery for Modified<&T>
where
    T: Sync + 'static,
{
    type Query = Self;

    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

unsafe impl<T> Query for Modified<&T>
where
    T: Sync + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = ModifiedFetchRead<'a, T>;

    const MUTABLE: bool = false;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <&T as PhantomQuery>::access(ty)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => false,
            Some(component) => unsafe {
                debug_assert_eq!(<&T as PhantomQuery>::visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data();
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> ModifiedFetchRead<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        let data = component.data();

        debug_assert!(data.epoch.after(self.after_epoch));

        ModifiedFetchRead {
            after_epoch: self.after_epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_ptr() as *mut EpochId),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Modified<&T> where T: Sync + 'static {}

impl<T> IntoQuery for Modified<Option<&T>>
where
    T: Sync + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self {
        self
    }
}

unsafe impl<T> Query for Modified<Option<&T>>
where
    T: Sync + 'static,
{
    type Item<'a> = Option<&'a T>;
    type Fetch<'a> = Option<ModifiedFetchRead<'a, T>>;

    const MUTABLE: bool = false;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <&T as PhantomQuery>::access(ty)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(<&T as PhantomQuery>::visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data();
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        if let Some(component) = archetype.component(TypeId::of::<T>()) {
            debug_assert_eq!(<&T as PhantomQuery>::visit_archetype(archetype), true);

            debug_assert_eq!(component.id(), TypeId::of::<T>());
            let data = component.data();
            if data.epoch.after(self.after_epoch) {
                f(TypeId::of::<T>(), Access::Read)
            }
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> Option<ModifiedFetchRead<'a, T>> {
        match archetype.component(TypeId::of::<T>()) {
            None => None,
            Some(component) => {
                let data = component.data();

                debug_assert!(data.epoch.after(self.after_epoch));

                Some(ModifiedFetchRead {
                    after_epoch: self.after_epoch,
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

unsafe impl<T> ImmutableQuery for Modified<Option<&T>> where T: Sync + 'static {}
