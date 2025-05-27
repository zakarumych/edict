use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        option::OptionQuery, write::Write, Access, AsQuery, Fetch, IntoQuery, Query, SendQuery,
        WriteAlias,
    },
    system::QueryArg,
    type_id,
    world::World,
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
    T: 'a,
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
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        let chunk_epoch = unsafe { *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let epoch = unsafe { *self.entity_epochs.as_ptr().add(idx as usize) };
        epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_epoch.bump(self.epoch);
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> &'a mut T {
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
        entity_epoch.bump(self.epoch);

        unsafe { &mut *self.ptr.as_ptr().add(idx as usize) }
    }
}

impl<T> AsQuery for Modified<&mut T>
where
    T: 'static,
{
    type Query = Modified<Write<T>>;
}

impl<T> AsQuery for Modified<Write<T>>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Modified<Write<T>>
where
    T: 'static,
{
    fn into_query(self) -> Self {
        self
    }
}

impl<T> QueryArg for Modified<Write<T>>
where
    T: Send + 'static,
{
    #[inline]
    fn new() -> Self {
        Modified {
            after_epoch: EpochId::start(),
            query: Write,
        }
    }

    #[inline]
    fn after(&mut self, world: &World) {
        self.after_epoch = world.epoch();
    }
}

unsafe impl<T> Query for Modified<Write<T>>
where
    T: 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = ModifiedFetchWrite<'a, T>;

    const MUTABLE: bool = true;

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        self.query.component_access(comp)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(type_id::<T>()) {
            None => false,
            Some(component) => unsafe {
                debug_assert_eq!(self.query.visit_archetype(archetype), true);
                debug_assert_eq!(component.id(), type_id::<T>());
                true
            },
        }
    }

    #[inline]
    unsafe fn visit_archetype_late(&self, archetype: &Archetype) -> bool {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        let data = unsafe { component.data() };
        data.epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(type_id::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> ModifiedFetchWrite<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        let data = unsafe { component.data_mut() };

        debug_assert!(data.epoch.after(self.after_epoch));
        data.epoch.bump(epoch);

        ModifiedFetchWrite {
            after_epoch: self.after_epoch,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
            chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
            marker: PhantomData,
        }
    }
}

unsafe impl<T> SendQuery for Modified<Write<T>> where T: Send + 'static {}

impl<T> AsQuery for Modified<Option<&mut T>>
where
    T: 'static,
{
    type Query = Modified<OptionQuery<Write<T>>>;
}

impl<T> AsQuery for Modified<OptionQuery<Write<T>>>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Modified<OptionQuery<Write<T>>>
where
    T: 'static,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

unsafe impl<T> Query for Modified<OptionQuery<Write<T>>>
where
    T: 'static,
{
    type Item<'a> = Option<&'a mut T>;
    type Fetch<'a> = Option<ModifiedFetchWrite<'a, T>>;

    const MUTABLE: bool = true;

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        self.query.component_access(comp)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(type_id::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(self.query.visit_archetype(archetype), true);
                debug_assert_eq!(component.id(), type_id::<T>());
                true
            },
        }
    }

    #[inline]
    unsafe fn visit_archetype_late(&self, archetype: &Archetype) -> bool {
        match archetype.component(type_id::<T>()) {
            None => true,
            Some(component) => unsafe {
                let data = unsafe { component.data() };
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if archetype.has_component(type_id::<T>()) {
            f(type_id::<T>(), Access::Write)
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Option<ModifiedFetchWrite<'a, T>> {
        match archetype.component(type_id::<T>()) {
            None => None,
            Some(component) => {
                let data = unsafe { component.data_mut() };

                debug_assert!(data.epoch.after(self.after_epoch));

                Some(ModifiedFetchWrite {
                    after_epoch: self.after_epoch,
                    epoch,
                    ptr: data.ptr.cast(),
                    entity_epochs: unsafe {
                        NonNull::new_unchecked(data.entity_epochs.as_mut_ptr())
                    },
                    chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
                    marker: PhantomData,
                })
            }
        }
    }
}

unsafe impl<T> SendQuery for Modified<OptionQuery<Write<T>>> where T: Send + 'static {}
