use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{
        copied::Copied, phantom::PhantomQuery, Access, Fetch, ImmutableQuery, IntoQuery, Query,
    },
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::{Modified, ModifiedCache};

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
    T: Copy + Sync + 'a,
{
    type Item = T;

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchCopied {
            after_epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) -> bool {
        let chunk_epoch = *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: usize) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx);
        epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> T {
        *self.ptr.as_ptr().add(idx)
    }
}

impl<T> IntoQuery for Modified<Copied<T>>
where
    T: Copy + Sync + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self {
        self
    }
}

unsafe impl<T> Query for Modified<Copied<T>>
where
    T: Copy + Sync + 'static,
{
    type Item<'a> = T;
    type Fetch<'a> = ModifiedFetchCopied<'a, T>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Copied<T> as PhantomQuery>::access(ty)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => false,
            Some(component) => unsafe {
                debug_assert_eq!(
                    <Copied<T> as PhantomQuery>::visit_archetype(archetype),
                    true
                );

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
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> ModifiedFetchCopied<'a, T> {
        debug_assert_ne!(
            archetype.len(),
            0,
            "Empty archetypes must be visited or skipped"
        );

        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
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

unsafe impl<T> ImmutableQuery for Modified<Copied<T>> where T: Copy + Sync + 'static {}

impl<'a, T> QueryArgGet<'a> for ModifiedCache<Copied<T>>
where
    T: Copy + Sync + 'static,
{
    type Arg = Modified<Copied<T>>;
    type Query = Modified<Copied<T>>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<Copied<T>> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<Copied<T>>
where
    T: Copy + Sync + 'static,
{
    fn new() -> Self {
        ModifiedCache {
            after_epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    fn access_component(&self, id: TypeId) -> Option<Access> {
        <Copied<T> as PhantomQuery>::access(id)
    }

    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        <Copied<T> as PhantomQuery>::visit_archetype(archetype)
    }
}

impl<'a, T> QueryArg for Modified<Copied<T>>
where
    T: Copy + Sync + 'static,
{
    type Cache = ModifiedCache<Copied<T>>;
}

impl<T> IntoQuery for Modified<Option<Copied<T>>>
where
    T: Copy + Sync + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self {
        self
    }
}

unsafe impl<T> Query for Modified<Option<Copied<T>>>
where
    T: Copy + Sync + 'static,
{
    type Item<'a> = Option<T>;
    type Fetch<'a> = Option<ModifiedFetchCopied<'a, T>>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Copied<T> as PhantomQuery>::access(ty)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(
                    <Copied<T> as PhantomQuery>::visit_archetype(archetype),
                    true
                );

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data();
                data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        if let Some(component) = archetype.component(TypeId::of::<T>()) {
            debug_assert_eq!(
                <Copied<T> as PhantomQuery>::visit_archetype(archetype),
                true
            );

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
        _epoch: EpochId,
    ) -> Option<ModifiedFetchCopied<'a, T>> {
        match archetype.component(TypeId::of::<T>()) {
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

unsafe impl<T> ImmutableQuery for Modified<Option<Copied<T>>> where T: Copy + Sync + 'static {}

impl<'a, T> QueryArgGet<'a> for ModifiedCache<Option<Copied<T>>>
where
    T: Copy + Sync + 'static,
{
    type Arg = Modified<Option<Copied<T>>>;
    type Query = Modified<Option<Copied<T>>>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<Option<Copied<T>>> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<Option<Copied<T>>>
where
    T: Copy + Sync + 'static,
{
    fn new() -> Self {
        ModifiedCache {
            after_epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    fn access_component(&self, id: TypeId) -> Option<Access> {
        <Copied<T> as PhantomQuery>::access(id)
    }

    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }
}

impl<'a, T> QueryArg for Modified<Option<Copied<T>>>
where
    T: Copy + Sync + 'static,
{
    type Cache = ModifiedCache<Option<Copied<T>>>;
}
