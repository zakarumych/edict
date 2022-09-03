use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::{AtomicBorrow, AtomicBorrowMut};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, IntoQuery, PhantomQuery, PhantomQueryFetch},
};

/// Query that borrows from components.
pub struct QueryBorrowAny<T> {
    marker: PhantomData<fn() -> T>,
}

/// Fetch for [`QueryBorrowAny<&T>`].
pub struct FetchBorrowAnyRead<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
    _borrow: AtomicBorrow<'a>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAnyRead<'a, T>
where
    T: Sync + ?Sized + 'a,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        FetchBorrowAnyRead {
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

impl<'a, T> PhantomQueryFetch<'a> for QueryBorrowAny<&T>
where
    T: Sync + ?Sized + 'a,
{
    type Item = &'a T;
    type Fetch = FetchBorrowAnyRead<'a, T>;
}

impl<T> IntoQuery for QueryBorrowAny<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Query = PhantomData<fn() -> Self>;
}

impl<T> PhantomQuery for QueryBorrowAny<&T>
where
    T: Sync + ?Sized + 'static,
{
    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_borrow(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _epoch: EpochId) -> FetchBorrowAnyRead<T> {
        let (cidx, bidx) = *archetype
            .borrow_indices(TypeId::of::<T>())
            .unwrap_unchecked()
            .get_unchecked(0);

        let component = archetype.component(cidx);
        debug_assert_eq!(component.borrows()[bidx].target(), TypeId::of::<T>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FetchBorrowAnyRead {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: component.borrows()[bidx].borrow(),
            _borrow: borrow,
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for QueryBorrowAny<&T> where T: Sync + ?Sized + 'static {}

/// Fetch for [`QueryBorrowAny<&mut T>`].
pub struct FetchBorrowAnyWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a mut ()>) -> &'a mut T,
    marker: PhantomData<fn() -> T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    _borrow: AtomicBorrowMut<'a>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAnyWrite<'a, T>
where
    T: Send + ?Sized + 'static,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchBorrowAnyWrite {
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
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_epoch.bump(self.epoch);
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

impl<'a, T> PhantomQueryFetch<'a> for QueryBorrowAny<&mut T>
where
    T: Send + ?Sized + 'static,
{
    type Item = &'a mut T;
    type Fetch = FetchBorrowAnyWrite<'a, T>;
}

impl<T> IntoQuery for QueryBorrowAny<&mut T>
where
    T: Send + ?Sized + 'static,
{
    type Query = PhantomData<fn() -> Self>;
}

impl<T> PhantomQuery for QueryBorrowAny<&mut T>
where
    T: Send + ?Sized + 'static,
{
    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_borrow_mut(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, epoch: EpochId) -> FetchBorrowAnyWrite<T> {
        let (cidx, bidx) = *archetype
            .borrow_mut_indices(TypeId::of::<T>())
            .unwrap_unchecked()
            .get_unchecked(0);

        let component = archetype.component(cidx);
        debug_assert_eq!(component.borrows()[bidx].target(), TypeId::of::<T>());

        let mut data = component.data.borrow_mut();

        data.epoch.bump(epoch);

        let (data, borrow) = atomicell::RefMut::into_split(data);

        FetchBorrowAnyWrite {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: component.borrows()[bidx].borrow_mut().unwrap_unchecked(),
            marker: PhantomData,
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            epoch,
            _borrow: borrow,
        }
    }
}
