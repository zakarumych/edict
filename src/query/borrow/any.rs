use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    query::{Access, Fetch, ImmutablePhantomQuery, PhantomQuery, PhantomQueryFetch, Query},
};

/// Query that borrows from components.
#[allow(missing_debug_implementations)]
pub struct QueryBorrowAny<T> {
    marker: PhantomData<fn() -> T>,
}

/// Fetch for [`QueryBorrowAny<&T>`].
#[allow(missing_debug_implementations)]
pub struct FetchBorrowAnyRead<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAnyRead<'a, T>
where
    T: ?Sized + 'static,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        FetchBorrowAnyRead {
            ptr: NonNull::dangling(),
            size: 0,
            borrow: |_, _| unreachable!(),
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
        (self.borrow)(
            NonNull::new_unchecked(self.ptr.as_ptr().add(idx * self.size)),
            PhantomData::<&'a ()>,
        )
    }
}

impl<'a, T: ?Sized + 'static> PhantomQueryFetch<'a> for QueryBorrowAny<&T> {
    type Item = &'a T;
    type Fetch = FetchBorrowAnyRead<'a, T>;
}

unsafe impl<T> PhantomQuery for QueryBorrowAny<&T>
where
    T: ?Sized + 'static,
{
    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn access_any() -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn conflicts<Q>(query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(query.access_any(), Some(Access::Write))
    }

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_borrow(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _epoch: u64) -> FetchBorrowAnyRead<T> {
        let (cidx, bidx) = *archetype
            .borrow_indices(TypeId::of::<T>())
            .unwrap_unchecked()
            .get_unchecked(0);

        let data = archetype.data(cidx);
        debug_assert_eq!(data.borrows()[bidx].target(), TypeId::of::<T>());

        FetchBorrowAnyRead {
            ptr: data.ptr,
            size: data.layout().size(),
            borrow: data.borrows()[bidx].borrow(),
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for QueryBorrowAny<&T> where T: ?Sized + 'static {}

/// Fetch for [`QueryBorrowAny<&mut T>`].
#[allow(missing_debug_implementations)]
pub struct FetchBorrowAnyWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow: unsafe fn(NonNull<u8>, PhantomData<&'a mut ()>) -> &'a mut T,
    marker: PhantomData<fn() -> T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
    epoch: u64,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAnyWrite<'a, T>
where
    T: ?Sized + 'static,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchBorrowAnyWrite {
            ptr: NonNull::dangling(),
            size: 0,
            borrow: |_, _| unreachable!(),
            marker: PhantomData,
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
            epoch: 0,
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

        (self.borrow)(
            NonNull::new_unchecked(self.ptr.as_ptr().add(idx * self.size)),
            PhantomData::<&'a mut ()>,
        )
    }
}

impl<'a, T: ?Sized + 'static> PhantomQueryFetch<'a> for QueryBorrowAny<&mut T> {
    type Item = &'a mut T;
    type Fetch = FetchBorrowAnyWrite<'a, T>;
}

unsafe impl<T> PhantomQuery for QueryBorrowAny<&mut T>
where
    T: ?Sized + 'static,
{
    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        Some(Access::Read)
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
        matches!(query.access_any(), Some(Access::Read | Access::Write))
    }

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_borrow_mut(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, epoch: u64) -> FetchBorrowAnyWrite<T> {
        let (cidx, bidx) = *archetype
            .borrow_mut_indices(TypeId::of::<T>())
            .unwrap_unchecked()
            .get_unchecked(0);

        let data = archetype.data(cidx);
        debug_assert_eq!(data.borrows()[bidx].target(), TypeId::of::<T>());

        debug_assert!(*data.version.get() < epoch);
        *data.version.get() = epoch;

        FetchBorrowAnyWrite {
            ptr: data.ptr,
            size: data.layout().size(),
            borrow: data.borrows()[bidx].borrow_mut().unwrap_unchecked(),
            marker: PhantomData,
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions,
            epoch,
        }
    }
}
