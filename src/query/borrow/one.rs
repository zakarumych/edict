use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    query::{Access, Fetch, ImmutableQuery, Query, QueryFetch},
};

/// Query that fetches components with specific `TypeId` as specified borrow.
#[allow(missing_debug_implementations)]
pub struct QueryBorrowOne<T> {
    id: TypeId,
    marker: PhantomData<fn() -> T>,
}

impl<T> QueryBorrowOne<T> {
    /// Construct a new query that fetches component with specified id.
    /// Borrowing it as `T`.
    pub fn new(id: TypeId) -> Self {
        QueryBorrowOne {
            id,
            marker: PhantomData,
        }
    }
}

/// Fetch for [`QueryBorrowOne<&T>`].
#[allow(missing_debug_implementations)]
pub struct FetchBorrowOneRead<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowOneRead<'a, T>
where
    T: ?Sized + 'static,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        FetchBorrowOneRead {
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

impl<'a, T> QueryFetch<'a> for QueryBorrowOne<&T>
where
    T: ?Sized + 'static,
{
    type Item = &'a T;
    type Fetch = FetchBorrowOneRead<'a, T>;
}

unsafe impl<T> Query for QueryBorrowOne<&T>
where
    T: ?Sized + 'static,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == self.id {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn conflicts<Q>(&self, query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(query.access(self.id), Some(Access::Write))
    }

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(self.id)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: u64,
    ) -> FetchBorrowOneRead<'a, T> {
        let idx = archetype.id_index(self.id).unwrap_unchecked();
        let data = archetype.data(idx);

        let cb = data
            .borrows()
            .iter()
            .find(|&cb| cb.target() == TypeId::of::<T>())
            .unwrap();

        FetchBorrowOneRead {
            ptr: data.ptr,
            size: data.layout().size(),
            borrow: cb.borrow(),
        }
    }
}

unsafe impl<T> ImmutableQuery for QueryBorrowOne<&T> where T: ?Sized + 'static {}

/// Fetch for [`QueryBorrowOne<&mut T>`].
#[allow(missing_debug_implementations)]
pub struct FetchBorrowOneWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow: unsafe fn(NonNull<u8>, PhantomData<&'a mut ()>) -> &'a mut T,
    marker: PhantomData<fn() -> T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
    epoch: u64,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowOneWrite<'a, T>
where
    T: ?Sized + 'static,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchBorrowOneWrite {
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

impl<'a, T> QueryFetch<'a> for QueryBorrowOne<&mut T>
where
    T: ?Sized + 'static,
{
    type Item = &'a mut T;
    type Fetch = FetchBorrowOneWrite<'a, T>;
}

unsafe impl<T> Query for QueryBorrowOne<&mut T>
where
    T: ?Sized + 'static,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == self.id {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn conflicts<Q>(&self, query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(query.access(self.id), Some(Access::Read | Access::Write))
    }

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(self.id)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: u64,
    ) -> FetchBorrowOneWrite<'a, T> {
        let idx = archetype.id_index(self.id).unwrap_unchecked();
        let data = archetype.data(idx);

        let cb = data
            .borrows()
            .iter()
            .find(|&cb| cb.target() == TypeId::of::<T>())
            .unwrap();

        assert!(cb.borrow_mut::<T>().is_some());

        debug_assert!(*data.version.get() < epoch);
        *data.version.get() = epoch;

        FetchBorrowOneWrite {
            ptr: data.ptr,
            size: data.layout().size(),
            borrow: cb.borrow_mut().unwrap_unchecked(),
            marker: PhantomData,
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions,
            epoch,
        }
    }
}
