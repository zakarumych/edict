use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{read::Read, write::Write, Access, Fetch, ImmutableQuery, IntoQuery, Query},
};

/// [`Query`] that fetches components with specific `TypeId` as specified borrow.
pub struct QueryBorrowOne<T> {
    id: TypeId,
    marker: PhantomData<T>,
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

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowOneRead {
            ptr: NonNull::dangling(),
            size: 0,
            borrow_fn: |_, _| unreachable!(),
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> &'a T {
        (self.borrow_fn)(
            NonNull::new_unchecked(self.ptr.as_ptr().add(idx as usize * self.size)),
            PhantomData::<&'a ()>,
        )
    }
}

impl<T> IntoQuery for QueryBorrowOne<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Query = QueryBorrowOne<Read<T>>;

    fn into_query(self) -> Self::Query {
        QueryBorrowOne {
            id: self.id,
            marker: PhantomData,
        }
    }
}

impl<T> IntoQuery for QueryBorrowOne<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for QueryBorrowOne<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchBorrowOneRead<'a, T>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == self.id {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(self.id)
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(self.id, Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
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

unsafe impl<T> ImmutableQuery for QueryBorrowOne<Read<T>> where T: Sync + ?Sized + 'static {}

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

    #[inline(always)]
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

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_version = &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize);
        chunk_version.bump(self.epoch);
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> &'a mut T {
        let entity_version = &mut *self.entity_epochs.as_ptr().add(idx as usize);
        entity_version.bump(self.epoch);

        (self.borrow_fn)(
            NonNull::new_unchecked(self.ptr.as_ptr().add(idx as usize * self.size)),
            PhantomData::<&'a mut ()>,
        )
    }
}

impl<T> IntoQuery for QueryBorrowOne<&mut T>
where
    T: Send + ?Sized + 'static,
{
    type Query = QueryBorrowOne<Write<T>>;

    fn into_query(self) -> Self::Query {
        QueryBorrowOne {
            id: self.id,
            marker: PhantomData,
        }
    }
}

impl<T> IntoQuery for QueryBorrowOne<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for QueryBorrowOne<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchBorrowOneWrite<'a, T>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == self.id {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(self.id)
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(self.id, Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
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
