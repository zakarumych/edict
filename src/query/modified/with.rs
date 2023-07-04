use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{filter::With, Access, Fetch, ImmutableQuery, IntoQuery, Query},
    system::QueryArg,
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
        let chunk_epoch = *self.chunk_epochs.as_ptr().add(chunk_idx as usize);
        chunk_epoch.after(self.after_epoch)
    }

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx as usize);
        epoch.after(self.after_epoch)
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, _: u32) {}
}

impl<T> IntoQuery for Modified<With<T>>
where
    T: 'static,
{
    type Query = Self;

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
            marker: PhantomData,
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
    fn access(&self, ty: TypeId) -> Option<Access> {
        With::<T>.access(ty)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => false,
            Some(component) => unsafe {
                debug_assert_eq!(With::<T>.visit_archetype(archetype), true);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data();
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> ModifiedFetchWith<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        let data = component.data();

        debug_assert!(data.epoch.after(self.after_epoch));

        ModifiedFetchWith {
            after_epoch: self.after_epoch,
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_ptr() as *mut EpochId),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Modified<With<T>> where T: 'static {}
