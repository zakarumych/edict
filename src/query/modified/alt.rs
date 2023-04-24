use core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{chunk_idx, Archetype},
    epoch::EpochId,
    query::{
        alt::{Alt, RefMut},
        phantom::PhantomQuery,
        Access, Fetch, IntoQuery, Query,
    },
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::{Modified, ModifiedCache};

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
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) -> bool {
        let epoch = &*self.chunk_epochs.as_ptr().add(chunk_idx);
        epoch.get().after(self.after_epoch)
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: usize) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx);
        epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RefMut<'a, T> {
        let archetype_epoch = &mut *self.archetype_epoch.as_ptr();
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx(idx));
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx);

        debug_assert!(entity_epoch.before(self.epoch));

        RefMut {
            component: &mut *self.ptr.as_ptr().add(idx),
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
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
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

impl<'a, T> QueryArgGet<'a> for ModifiedCache<Alt<T>>
where
    T: Send + 'static,
{
    type Arg = Modified<Alt<T>>;
    type Query = Modified<Alt<T>>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<Alt<T>> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<Alt<T>>
where
    T: Send + 'static,
{
    fn new() -> Self {
        ModifiedCache {
            after_epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    fn access_component(&self, id: TypeId) -> Option<Access> {
        <Alt<T> as PhantomQuery>::access(id)
    }

    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        <Alt<T> as PhantomQuery>::visit_archetype(archetype)
    }
}

impl<'a, T> QueryArg for Modified<Alt<T>>
where
    T: Send + 'static,
{
    type Cache = ModifiedCache<Alt<T>>;
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
    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
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
        &mut self,
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

impl<'a, T> QueryArgGet<'a> for ModifiedCache<Option<Alt<T>>>
where
    T: Send + 'static,
{
    type Arg = Modified<Option<Alt<T>>>;
    type Query = Modified<Option<Alt<T>>>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<Option<Alt<T>>> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<Option<Alt<T>>>
where
    T: Send + 'static,
{
    fn new() -> Self {
        ModifiedCache {
            after_epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    fn access_component(&self, id: TypeId) -> Option<Access> {
        <Alt<T> as PhantomQuery>::access(id)
    }

    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }
}

impl<'a, T> QueryArg for Modified<Option<Alt<T>>>
where
    T: Send + 'static,
{
    type Cache = ModifiedCache<Option<Alt<T>>>;
}
