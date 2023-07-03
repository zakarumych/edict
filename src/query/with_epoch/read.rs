use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{phantom::PhantomQuery, Access, Fetch, ImmutableQuery, IntoQuery, Query},
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::WithEpoch;

/// [`Fetch`] type for the [`WithEpoch<&T>`] query.
pub struct WithEpochFetchRead<'a, T> {
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for WithEpochFetchRead<'a, T>
where
    T: Sync + 'a,
{
    type Item = (&'a T, EpochId);

    #[inline(always)]
    fn dangling() -> Self {
        WithEpochFetchRead {
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: usize) -> (&'a T, EpochId) {
        let epoch = *self.entity_epochs.as_ptr().add(idx);
        (&*self.ptr.as_ptr().add(idx), epoch)
    }
}

impl<T> IntoQuery for WithEpoch<&T>
where
    T: Sync + 'static,
{
    type Query = PhantomData<fn() -> Self>;

    #[inline(always)]
    fn into_query(self) -> Self::Query {
        PhantomData
    }
}

unsafe impl<T> PhantomQuery for WithEpoch<&T>
where
    T: Sync + 'static,
{
    type Item<'a> = (&'a T, EpochId);
    type Fetch<'a> = WithEpochFetchRead<'a, T>;

    #[inline(always)]
    fn access(ty: TypeId) -> Option<Access> {
        <&T as PhantomQuery>::access(ty)
    }

    #[inline(always)]
    fn visit_archetype(archetype: &Archetype) -> bool {
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

    #[inline(always)]
    unsafe fn access_archetype(_archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: EpochId) -> WithEpochFetchRead<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        let data = component.data();

        WithEpochFetchRead {
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for WithEpoch<&T> where T: Sync + 'static {}

impl<T> IntoQuery for WithEpoch<Option<&T>>
where
    T: Sync + 'static,
{
    type Query = PhantomData<fn() -> Self>;

    fn into_query(self) -> Self::Query {
        PhantomData
    }
}

unsafe impl<T> Query for WithEpoch<Option<&T>>
where
    T: Sync + 'static,
{
    type Item<'a> = Option<(&'a T, EpochId)>;
    type Fetch<'a> = Option<WithEpochFetchRead<'a, T>>;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <&T as PhantomQuery>::access(ty)
    }

    #[inline(always)]
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

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        if let Some(component) = archetype.component(TypeId::of::<T>()) {
            debug_assert_eq!(<&T as PhantomQuery>::visit_archetype(archetype), true);

            debug_assert_eq!(component.id(), TypeId::of::<T>());
            let data = component.data();
            if data.epoch.after(self.after_epoch) {
                f(TypeId::of::<T>(), Access::Read)
            }
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> Option<WithEpochFetchRead<'a, T>> {
        match archetype.component(TypeId::of::<T>()) {
            None => None,
            Some(component) => {
                let data = component.data();

                debug_assert!(data.epoch.after(self.after_epoch));

                Some(WithEpochFetchRead {
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

unsafe impl<T> ImmutableQuery for WithEpoch<Option<&T>> where T: Sync + 'static {}

impl<'a, T> QueryArgGet<'a> for ModifiedCache<Option<&T>>
where
    T: Sync + 'static,
{
    type Arg = WithEpoch<Option<&'a T>>;
    type Query = WithEpoch<Option<&'a T>>;

    #[inline(always)]
    fn get(&mut self, world: &'a World) -> WithEpoch<Option<&T>> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        WithEpoch {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<Option<&T>>
where
    T: Sync + 'static,
{
    fn new() -> Self {
        ModifiedCache {
            after_epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    fn access_component(&self, id: TypeId) -> Option<Access> {
        <&T as PhantomQuery>::access(id)
    }

    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }
}

impl<'a, T> QueryArg for WithEpoch<Option<&T>>
where
    T: Sync + 'static,
{
    type Cache = ModifiedCache<Option<&'static T>>;
}
