use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, component::Component};

use super::{phantom::PhantomQuery, Access, Fetch, ImmutablePhantomQuery, Query};

/// `Fetch` type for the `&T` query.
#[allow(missing_debug_implementations)]

pub struct FetchRead<T> {
    pub(super) ptr: NonNull<T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchRead<T>
where
    T: Component,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        FetchRead {
            ptr: NonNull::dangling(),
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
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a T {
        &*self.ptr.as_ptr().add(idx)
    }
}

unsafe impl<T> PhantomQuery for &T
where
    T: Component,
{
    type Fetch = FetchRead<T>;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn conflicts<Q>(query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(query.access(TypeId::of::<T>()), Some(Access::Write))
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
    unsafe fn fetch(archetype: &Archetype, _epoch: u64) -> FetchRead<T> {
        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<T>());

        FetchRead {
            ptr: data.ptr.cast(),
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for &T where T: Component {}

/// Returns query that yields reference to specified component
/// for each entity that has that component.
///
/// Skips entities that don't have the component.
pub fn read<'a, T>() -> PhantomData<&'a T>
where
    T: Component,
{
    PhantomData
}
