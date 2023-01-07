use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, Fetch, IntoQuery, QueryFetch},
    system::{QueryArg, QueryArgCache, QueryArgGet},
    Modified, PhantomQuery, Query, World,
};

use super::ModifiedCache;

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

    #[inline]
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

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let chunk_epoch = *self.chunk_epochs.as_ptr().add(chunk_idx);
        !chunk_epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx);
        !epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_epoch.bump(self.epoch);
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a mut T {
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx);
        entity_epoch.bump(self.epoch);

        &mut *self.ptr.as_ptr().add(idx)
    }
}

impl<'a, T> QueryFetch<'a> for Modified<&mut T>
where
    T: Send + 'a,
{
    type Item = &'a mut T;
    type Fetch = ModifiedFetchWrite<'a, T>;
}

impl<T> IntoQuery for Modified<&mut T>
where
    T: Send + 'static,
{
    type Query = Self;
}

unsafe impl<T> Query for Modified<&mut T>
where
    T: Send + 'static,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <&mut T as PhantomQuery>::access(ty)
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(<&mut T as PhantomQuery>::skip_archetype(archetype), false);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data_mut();
                !data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> ModifiedFetchWrite<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

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

impl<'a, T> QueryArgGet<'a> for ModifiedCache<&'static mut T>
where
    T: Send + 'static,
{
    type Arg = Modified<&'a mut T>;
    type Query = Modified<&'a mut T>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<&'a mut T> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<&'static mut T>
where
    T: Send + 'static,
{
    fn access_component(&self, id: TypeId) -> Option<Access> {
        <&mut T as PhantomQuery>::access(id)
    }

    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        <&mut T as PhantomQuery>::skip_archetype(archetype)
    }
}

impl<'a, T> QueryArg for Modified<&'a mut T>
where
    T: Send + 'static,
{
    type Cache = ModifiedCache<&'static mut T>;
}
