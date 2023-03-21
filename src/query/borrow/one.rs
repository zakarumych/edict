use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query},
};

/// [`Query`] that fetches components with specific `TypeId` as specified borrow.
pub struct QueryBorrowOne<T> {
    id: TypeId,
    marker: PhantomData<fn() -> T>,
}

impl<T> Copy for QueryBorrowOne<T> {}

impl<T> Clone for QueryBorrowOne<T> {
    fn clone(&self) -> Self {
        *self
    }

    fn clone_from(&mut self, source: &Self) {
        self.id = source.id;
    }
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

/// [`Fetch`] for [`QueryBorrowOne<&T>`].
pub struct FetchBorrowOneRead<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
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
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a T {
        (self.borrow_fn)(
            NonNull::new_unchecked(self.ptr.as_ptr().add(idx * self.size)),
            PhantomData::<&'a ()>,
        )
    }
}

impl<T> IntoQuery for QueryBorrowOne<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for QueryBorrowOne<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchBorrowOneRead<'a, T>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == self.id {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(self.id)
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(self.id, Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchBorrowOneRead<'a, T> {
        let component = archetype.component(self.id).unwrap_unchecked();

        let cb = component
            .borrows()
            .iter()
            .find(|&cb| cb.target() == TypeId::of::<T>())
            .unwrap();

        let data = component.data();

        FetchBorrowOneRead {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: cb.borrow(),
        }
    }
}

unsafe impl<T> ImmutableQuery for QueryBorrowOne<&T> where T: Sync + ?Sized + 'static {}

/// [`Fetch`] for [`QueryBorrowOne<&mut T>`].
pub struct FetchBorrowOneWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a mut ()>) -> &'a mut T,
    marker: PhantomData<fn() -> T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
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
        }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: usize) {
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

impl<T> IntoQuery for QueryBorrowOne<&mut T>
where
    T: Send + ?Sized + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for QueryBorrowOne<&mut T>
where
    T: Send + ?Sized + 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchBorrowOneWrite<'a, T>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == self.id {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(self.id)
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(self.id, Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchBorrowOneWrite<'a, T> {
        let component = archetype.component(self.id).unwrap_unchecked();

        let cb = component
            .borrows()
            .iter()
            .find(|&cb| cb.target() == TypeId::of::<T>())
            .unwrap();

        assert!(cb.borrow_mut::<T>().is_some());

        let data = component.data_mut();

        data.epoch.bump(epoch);

        FetchBorrowOneWrite {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: cb.borrow_mut().unwrap_unchecked(),
            marker: PhantomData,
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            epoch,
        }
    }
}
