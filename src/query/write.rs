use core::{any::TypeId, ptr::NonNull};

use crate::{archetype::Archetype, component::Component};

use super::{Fetch, NonTrackingQuery, Query};

/// `Fetch` type for the `&mut T` query.
#[allow(missing_debug_implementations)]
pub struct FetchWrite<T> {
    epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
}

impl<'a, T> Fetch<'a> for FetchWrite<T>
where
    T: Component,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchWrite {
            epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
        }
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

impl<T> Query for &mut T
where
    T: Component,
{
    type Fetch = FetchWrite<T>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _tracks: u64, epoch: u64) -> Option<FetchWrite<T>> {
        let idx = archetype.id_index(TypeId::of::<T>())?;
        let data = archetype.data_mut(idx);
        debug_assert_eq!(data.id, TypeId::of::<T>());

        debug_assert!(data.version < epoch);
        data.version = epoch;

        Some(FetchWrite {
            epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions.cast(),
        })
    }
}

unsafe impl<T> NonTrackingQuery for &mut T {}
