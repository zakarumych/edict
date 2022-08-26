use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::{AtomicBorrow, AtomicBorrowMut};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query, QueryFetch},
};

/// Query that fetches components with specific `TypeId` as specified borrow.
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
pub struct FetchBorrowOneRead<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
    _borrow: AtomicBorrow<'a>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowOneRead<'a, T>
where
    T: Sync + ?Sized + 'a,
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
    T: Sync + ?Sized + 'a,
{
    type Item = &'a T;
    type Fetch = FetchBorrowOneRead<'a, T>;
}

impl<T> IntoQuery for QueryBorrowOne<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Query = Self;
}

unsafe impl<T> Query for QueryBorrowOne<&T>
where
    T: Sync + ?Sized + 'static,
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
        _epoch: EpochId,
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

unsafe impl<T> ImmutableQuery for QueryBorrowOne<&T> where T: Sync + ?Sized + 'static {}

/// Fetch for [`QueryBorrowOne<&mut T>`].
pub struct FetchBorrowOneWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a mut ()>) -> &'a mut T,
    marker: PhantomData<fn() -> T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    _borrow: AtomicBorrowMut<'a>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowOneWrite<'a, T>
where
    T: Send + ?Sized + 'a,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchBorrowOneWrite {
            ptr: NonNull::dangling(),
            size: 0,
            borrow_fn: |_, _| unreachable!(),
            marker: PhantomData,
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            epoch: EpochId::start(),
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
        let chunk_version = &mut *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_version.bump(self.epoch);
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a mut T {
        let entity_version = &mut *self.entity_epochs.as_ptr().add(idx);
        entity_version.bump(self.epoch);

        (self.borrow_fn)(
            NonNull::new_unchecked(self.ptr.as_ptr().add(idx * self.size)),
            PhantomData::<&'a mut ()>,
        )
    }
}

impl<'a, T> QueryFetch<'a> for QueryBorrowOne<&mut T>
where
    T: Send + ?Sized + 'a,
{
    type Item = &'a mut T;
    type Fetch = FetchBorrowOneWrite<'a, T>;
}

impl<T> IntoQuery for QueryBorrowOne<&mut T>
where
    T: Send + ?Sized + 'static,
{
    type Query = Self;
}

unsafe impl<T> Query for QueryBorrowOne<&mut T>
where
    T: Send + ?Sized + 'static,
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
        epoch: EpochId,
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

        data.epoch.bump(epoch);

        let (data, borrow) = atomicell::RefMut::into_split(data);

        FetchBorrowOneWrite {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: cb.borrow_mut().unwrap_unchecked(),
            marker: PhantomData,
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            epoch,
            _borrow: borrow,
        }
    }
}
