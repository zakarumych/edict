use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        filter::With, Access, AsQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery,
        WriteAlias,
    },
    system::QueryArg,
    type_id,
    world::World,
};

use super::Modified;

/// [`Fetch`] type for the [`Modified<&T>`] query.
pub struct ModifiedFetchWith<'a, T> {
    after_epoch: EpochId,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchWith<'a, T>
where
    T: 'a,
{
    type Item = ();

    #[inline(always)]
    fn dangling() -> Self {
        ModifiedFetchWith {
            after_epoch: EpochId::start(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        let chunk_epoch = unsafe { *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_epoch.after(self.after_epoch)
    }

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let epoch = unsafe { *self.entity_epochs.as_ptr().add(idx as usize) };
        epoch.after(self.after_epoch)
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, _: u32) {}
}

impl<T> AsQuery for Modified<With<T>>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Modified<With<T>>
where
    T: 'static,
{
    fn into_query(self) -> Self {
        self
    }
}

impl<T> QueryArg for Modified<With<T>>
where
    T: 'static,
{
    #[inline(always)]
    fn new() -> Self {
        Modified {
            after_epoch: EpochId::start(),
            query: With,
        }
    }

    #[inline(always)]
    fn after(&mut self, world: &World) {
        self.after_epoch = world.epoch();
    }
}

unsafe impl<T> Query for Modified<With<T>>
where
    T: 'static,
{
    type Item<'a> = ();
    type Fetch<'a> = ModifiedFetchWith<'a, T>;

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
                let data = unsafe { component.data() };
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
    ) -> ModifiedFetchWith<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        let data = unsafe { component.data() };

        debug_assert!(data.epoch.after(self.after_epoch));

        ModifiedFetchWith {
            after_epoch: self.after_epoch,
            entity_epochs: unsafe {
                NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId)
            },
            chunk_epochs: unsafe {
                NonNull::new_unchecked(data.chunk_epochs.as_ptr() as *mut EpochId)
            },
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Modified<With<T>> where T: 'static {}
unsafe impl<T> SendQuery for Modified<With<T>> where T: 'static {}
