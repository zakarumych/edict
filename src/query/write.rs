use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{assert_query, phantom::PhantomQuery, Access, Fetch, IntoQuery};

/// [`Fetch`] type for the `&mut T` query.
pub struct FetchWrite<'a, T> {
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchWrite<'a, T>
where
    T: Send + 'a,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchWrite {
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: usize) {
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

impl<T> IntoQuery for &mut T
where
    T: Send + 'static,
{
    type Query = PhantomData<fn() -> Self>;

    #[inline]
    fn into_query(self) -> Self::Query {
        PhantomData
    }
}

unsafe impl<T> PhantomQuery for &mut T
where
    T: Send + 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchWrite<'a, T>;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: EpochId) -> FetchWrite<'a, T> {
        debug_assert_ne!(
            archetype.len(),
            0,
            "Empty archetypes must be visited or skipped"
        );

        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data_mut();
        data.epoch.bump(epoch);

        FetchWrite {
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            epoch,
            marker: PhantomData,
        }
    }
}

/// Phantom data for the `&mut T` phantom query.
pub type Write<T> = PhantomData<fn() -> &'static mut T>;

/// Returns query that yields mutable reference to specified component
/// for each entity that has that component.
///
/// Skips entities that don't have the component.
pub fn write<T>() -> Write<T>
where
    T: Send,
{
    assert_query::<Write<T>>();
    PhantomData
}
