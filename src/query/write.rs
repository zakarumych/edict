use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::{borrow::AtomicBorrowMut, RefMut};

use crate::archetype::Archetype;

use super::{phantom::PhantomQuery, Access, Fetch, PhantomQueryFetch, Query};

/// `Fetch` type for the `&mut T` query.
#[allow(missing_debug_implementations)]
pub struct FetchWrite<'a, T> {
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
    epoch: u64,
    _borrow: AtomicBorrowMut<'a>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchWrite<'a, T>
where
    T: 'static,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchWrite {
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
            epoch: 0,
            _borrow: AtomicBorrowMut::dummy(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn skip_item(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx);

        debug_assert!(*chunk_version < self.epoch);
        *chunk_version = self.epoch;
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a mut T {
        let entity_version = &mut *self.entity_versions.as_ptr().add(idx);

        debug_assert!(*entity_version < self.epoch);
        *entity_version = self.epoch;

        &mut *self.ptr.as_ptr().add(idx)
    }
}

impl<'a, T> PhantomQueryFetch<'a> for &mut T
where
    T: 'static,
{
    type Item = &'a mut T;
    type Fetch = FetchWrite<'a, T>;
}

unsafe impl<T> PhantomQuery for &mut T
where
    T: 'static,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn access_any() -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn conflicts<Q>(query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(
            query.access(TypeId::of::<T>()),
            Some(Access::Read | Access::Write)
        )
    }

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: u64) -> FetchWrite<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let (data, borrow) = RefMut::into_split(component.data.borrow_mut());

        debug_assert!(data.version < epoch);
        data.version = epoch;

        FetchWrite {
            ptr: data.ptr.cast(),
            entity_versions: NonNull::new_unchecked(data.entity_versions.as_mut_ptr()),
            chunk_versions: NonNull::new_unchecked(data.chunk_versions.as_mut_ptr()),
            epoch,
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

/// Returns query that yields mutable reference to specified component
/// for each entity that has that component.
///
/// Skips entities that don't have the component.
pub fn write<'a, T>() -> PhantomData<&'a mut T>
where
    T: 'static,
{
    PhantomData
}
