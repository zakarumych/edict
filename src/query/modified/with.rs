use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{filter::With, phantom::PhantomQuery, Access, Fetch, ImmutableQuery, IntoQuery, Query},
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::{Modified, ModifiedCache};

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

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchWith {
            after_epoch: EpochId::start(),
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
    unsafe fn get_item(&mut self, _: usize) {}
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

unsafe impl<T> Query for Modified<With<T>>
where
    T: 'static,
{
    type Item<'a> = ();
    type Fetch<'a> = ModifiedFetchWith<'a, T>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <With<T> as PhantomQuery>::access(ty)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => false,
            Some(component) => unsafe {
                debug_assert_eq!(<With<T> as PhantomQuery>::visit_archetype(archetype), true);

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
    ) -> ModifiedFetchWith<'a, T> {
        debug_assert_ne!(
            archetype.len(),
            0,
            "Empty archetypes must be visited or skipped"
        );

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

impl<'a, T> QueryArgGet<'a> for ModifiedCache<With<T>>
where
    T: 'static,
{
    type Arg = Modified<With<T>>;
    type Query = Modified<With<T>>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<With<T>> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<With<T>>
where
    T: 'static,
{
    fn new() -> Self {
        ModifiedCache {
            after_epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    fn access_component(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }
}

impl<'a, T> QueryArg for Modified<With<T>>
where
    T: 'static,
{
    type Cache = ModifiedCache<With<T>>;
}
