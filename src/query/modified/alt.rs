use core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{chunk_idx, Archetype},
    epoch::EpochId,
    query::{
        alt::{Alt, RefMut},
        phantom::PhantomQuery,
        Access, Fetch, IntoQuery, Query,
    },
};

use super::Modified;

/// [`Fetch`] type for the [`Modified<Alt<T>>`] query.
pub struct ModifiedFetchAlt<'a, T> {
    after_epoch: EpochId,
    epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<Cell<EpochId>>,
    archetype_epoch: NonNull<Cell<EpochId>>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchAlt<'a, T>
where
    T: Send + 'a,
{
    type Item = RefMut<'a, T>;

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchAlt {
            after_epoch: EpochId::start(),
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            archetype_epoch: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        let epoch = &*self.chunk_epochs.as_ptr().add(chunk_idx as usize);
        epoch.get().after(self.after_epoch)
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx as usize);
        epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> RefMut<'a, T> {
        let archetype_epoch = &mut *self.archetype_epoch.as_ptr();
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx(idx) as usize);
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx as usize);

        debug_assert!(entity_epoch.before(self.epoch));

        RefMut {
            component: &mut *self.ptr.as_ptr().add(idx as usize),
            entity_epoch,
            chunk_epoch,
            archetype_epoch,
            epoch: self.epoch,
        }
    }
}

impl<T> IntoQuery for Modified<Alt<T>>
where
    T: Send + 'static,
{
    type Query = Self;

    #[inline]
    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for Modified<Alt<T>>
where
    T: Send + 'static,
{
    type Item<'a> = RefMut<'a, T>;
    type Fetch<'a> = ModifiedFetchAlt<'a, T>;

    const MUTABLE: bool = true;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Alt<T> as PhantomQuery>::access(ty)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => false,
            Some(component) => unsafe {
                debug_assert_eq!(<Alt<T> as PhantomQuery>::visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data_mut();
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> ModifiedFetchAlt<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data_mut();

        debug_assert!(data.epoch.after(self.after_epoch));
        debug_assert!(data.epoch.before(epoch));

        ModifiedFetchAlt {
            after_epoch: self.after_epoch,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()).cast(),
            archetype_epoch: NonNull::from(&mut data.epoch).cast(),
            marker: PhantomData,
        }
    }
}

impl<T> IntoQuery for Modified<Option<Alt<T>>>
where
    T: Send + 'static,
{
    type Query = Self;

    #[inline]
    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for Modified<Option<Alt<T>>>
where
    T: Send + 'static,
{
    type Item<'a> = Option<RefMut<'a, T>>;
    type Fetch<'a> = Option<ModifiedFetchAlt<'a, T>>;

    const MUTABLE: bool = true;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Alt<T> as PhantomQuery>::access(ty)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(<Alt<T> as PhantomQuery>::visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data();
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if let Some(component) = archetype.component(TypeId::of::<T>()) {
            debug_assert_eq!(<Alt<T> as PhantomQuery>::visit_archetype(archetype), true);

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
        epoch: EpochId,
    ) -> Option<ModifiedFetchAlt<'a, T>> {
        match archetype.component(TypeId::of::<T>()) {
            None => None,
            Some(component) => {
                let data = component.data_mut();

                debug_assert!(data.epoch.after(self.after_epoch));

                Some(ModifiedFetchAlt {
                    after_epoch: self.after_epoch,
                    epoch,
                    ptr: data.ptr.cast(),
                    entity_epochs: NonNull::new_unchecked(
                        data.entity_epochs.as_ptr() as *mut EpochId
                    ),
                    chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()).cast(),
                    archetype_epoch: NonNull::from(&mut data.epoch).cast(),
                    marker: PhantomData,
                })
            }
        }
    }
}
