use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        copied::Cpy, option::OptionQuery, Access, AsQuery, Fetch, ImmutableQuery, IntoQuery, Query,
        SendQuery, WriteAlias,
    },
    system::QueryArg,
    type_id,
    world::World,
};

use super::Modified;

/// [`Fetch`] type for the [`Modified<Copied<T>>`] query.
pub struct ModifiedFetchCopied<'a, T> {
    after_epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchCopied<'a, T>
where
    T: Copy + 'a,
{
    type Item = T;

    #[inline(always)]
    fn dangling() -> Self {
        ModifiedFetchCopied {
            after_epoch: EpochId::start(),
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
    unsafe fn get_item(&mut self, idx: u32) -> T {
        *self.ptr.as_ptr().add(idx as usize)
    }
}

impl<T> AsQuery for Modified<Cpy<T>>
where
    T: Copy + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Modified<Cpy<T>>
where
    T: Copy + 'static,
{
    fn into_query(self) -> Self {
        self
    }
}

impl<T> QueryArg for Modified<Cpy<T>>
where
    T: Copy + Sync + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        Modified {
            after_epoch: EpochId::start(),
            query: Cpy,
        }
    }

    #[inline(always)]
    fn after(&mut self, world: &World) {
        self.after_epoch = world.epoch();
    }
}

unsafe impl<T> Query for Modified<Cpy<T>>
where
    T: Copy + 'static,
{
    type Item<'a> = T;
    type Fetch<'a> = ModifiedFetchCopied<'a, T>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        self.query.component_access(comp)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(type_id::<T>()) {
            None => false,
            Some(component) => unsafe {
                debug_assert_eq!(self.query.visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), type_id::<T>());
                let data = component.data();
                data.epoch.after(self.after_epoch)
            },
        }
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
    ) -> ModifiedFetchCopied<'a, T> {
        let component = archetype.component(type_id::<T>()).unwrap_unchecked();
        let data = component.data();

        debug_assert!(data.epoch.after(self.after_epoch));

        ModifiedFetchCopied {
            after_epoch: self.after_epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_ptr() as *mut EpochId),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Modified<Cpy<T>> where T: Copy + 'static {}
unsafe impl<T> SendQuery for Modified<Cpy<T>> where T: Sync + Copy + 'static {}

impl<T> AsQuery for Modified<Option<Cpy<T>>>
where
    T: Copy + 'static,
{
    type Query = Modified<OptionQuery<Cpy<T>>>;
}

impl<T> AsQuery for Modified<OptionQuery<Cpy<T>>>
where
    T: Copy + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Modified<OptionQuery<Cpy<T>>>
where
    T: Copy + 'static,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> QueryArg for Modified<OptionQuery<Cpy<T>>>
where
    T: Copy + Sync + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        Modified {
            after_epoch: EpochId::start(),
            query: OptionQuery(Cpy),
        }
    }

    #[inline(always)]
    fn after(&mut self, world: &World) {
        self.after_epoch = world.epoch();
    }
}

unsafe impl<T> Query for Modified<OptionQuery<Cpy<T>>>
where
    T: Copy + 'static,
{
    type Item<'a> = Option<T>;
    type Fetch<'a> = Option<ModifiedFetchCopied<'a, T>>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        self.query.component_access(comp)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(type_id::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(self.query.visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), type_id::<T>());
                let data = component.data();
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if let Some(component) = archetype.component(type_id::<T>()) {
            debug_assert_eq!(self.query.visit_archetype(archetype), true);

            debug_assert_eq!(component.id(), type_id::<T>());
            let data = component.data();
            if data.epoch.after(self.after_epoch) {
                f(type_id::<T>(), Access::Read)
            }
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> Option<ModifiedFetchCopied<'a, T>> {
        match archetype.component(type_id::<T>()) {
            None => None,
            Some(component) => {
                let data = component.data();

                debug_assert!(data.epoch.after(self.after_epoch));

                Some(ModifiedFetchCopied {
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

unsafe impl<T> ImmutableQuery for Modified<OptionQuery<Cpy<T>>> where T: Copy + 'static {}
unsafe impl<T> SendQuery for Modified<OptionQuery<Cpy<T>>> where T: Sync + Copy + 'static {}
