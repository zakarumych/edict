use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::{AtomicBorrow, AtomicBorrowMut};

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
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
    _borrow: AtomicBorrow<'a>,
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
            borrow_fn: |_, _| unreachable!(),
            _borrow: AtomicBorrow::dummy(),
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
        (self.borrow_fn)(
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
        let component = archetype.component(idx);

        let cb = component
            .borrows()
            .iter()
            .find(|&cb| cb.target() == TypeId::of::<T>())
            .unwrap();

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FetchBorrowOneRead {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: cb.borrow(),
            _borrow: borrow,
        }
    }
}

unsafe impl<T> ImmutableQuery for QueryBorrowOne<&T> where T: ?Sized + 'static {}

/// Fetch for [`QueryBorrowOne<&mut T>`].
#[allow(missing_debug_implementations)]
pub struct FetchBorrowOneWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a mut ()>) -> &'a mut T,
    marker: PhantomData<fn() -> T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
    epoch: u64,
    _borrow: AtomicBorrowMut<'a>,
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
            borrow_fn: |_, _| unreachable!(),
            marker: PhantomData,
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
            epoch: 0,
            _borrow: AtomicBorrowMut::dummy(),
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

        (self.borrow_fn)(
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
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(self.id).unwrap_unchecked();
        let component = archetype.component(idx);

        let cb = component
            .borrows()
            .iter()
            .find(|&cb| cb.target() == TypeId::of::<T>())
            .unwrap();

        assert!(cb.borrow_mut::<T>().is_some());

        let mut data = component.data.borrow_mut();

        debug_assert!(data.version < epoch);
        data.version = epoch;

        let (data, borrow) = atomicell::RefMut::into_split(data);

        FetchBorrowOneWrite {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: cb.borrow_mut().unwrap_unchecked(),
            marker: PhantomData,
            entity_versions: NonNull::from(data.entity_versions.get_unchecked_mut(0)),
            chunk_versions: NonNull::from(data.chunk_versions.get_unchecked_mut(0)),
            epoch,
            _borrow: borrow,
        }
    }
}
