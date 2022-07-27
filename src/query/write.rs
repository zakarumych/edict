use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, component::Component};

use super::{phantom::PhantomQuery, Access, Fetch, Query};

/// `Fetch` type for the `&mut T` query.
#[allow(missing_debug_implementations)]
pub struct FetchWrite<T> {
    epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
}

unsafe impl<'a, T> Fetch<'a> for FetchWrite<T>
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

unsafe impl<T> PhantomQuery for &mut T
where
    T: Component,
{
    type Fetch = FetchWrite<T>;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Write)
        } else {
            None
        }
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
    unsafe fn fetch(archetype: &Archetype, epoch: u64) -> FetchWrite<T> {
        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<T>());

        debug_assert!(*data.version.get() < epoch);
        *data.version.get() = epoch;

        FetchWrite {
            epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions,
        }
    }
}

/// Returns query that yields mutable reference to specified component
/// for each entity that has that component.
///
/// Skips entities that don't have the component.
pub fn write<'a, T>() -> PhantomData<&'a mut T>
where
    T: Component,
{
    PhantomData
}
