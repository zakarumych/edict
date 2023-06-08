use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, PhantomQuery},
};

phantom_newtype! {
    /// [`PhantomQuery`] that borrows from components.
    pub struct QueryBorrowAny<T>
}

impl<T> QueryBorrowAny<&T>
where
    T: Sync + ?Sized + 'static,
{
    /// Creates a new [`QueryBorrowAny`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

impl<T> QueryBorrowAny<&mut T>
where
    T: Send + ?Sized + 'static,
{
    /// Creates a new [`QueryBorrowAny`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

/// [`Fetch`] for [`QueryBorrowAny<&T>`].
pub struct FetchBorrowAnyRead<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
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
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a T {
        unsafe {
            (self.borrow_fn)(
                NonNull::new_unchecked(self.ptr.as_ptr().add(idx * self.size)),
                PhantomData::<&'a ()>,
            )
        }
    }
}

unsafe impl<T> PhantomQuery for QueryBorrowAny<&'static T>
where
    T: Sync + ?Sized + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchBorrowAnyRead<'a, T>;

    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.contains_borrow(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn access_archetype(archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        for (id, _) in archetype
            .borrow_indices(TypeId::of::<T>())
            .unwrap_unchecked()
        {
            f(*id, Access::Read);
        }
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _epoch: EpochId) -> FetchBorrowAnyRead<T> {
        let (id, idx) = *archetype
            .borrow_indices(TypeId::of::<T>())
            .unwrap_unchecked()
            .get_unchecked(0);

        let component = archetype.component(id).unwrap_unchecked();
        debug_assert_eq!(component.borrows()[idx].target(), TypeId::of::<T>());

        let data = component.data();

        FetchBorrowAnyRead {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: component.borrows()[idx].borrow(),
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for QueryBorrowAny<&'static T> where T: Sync + ?Sized + 'static {}

/// [`Fetch`] for [`QueryBorrowAny<&mut T>`].
pub struct FetchBorrowAnyWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a mut ()>) -> &'a mut T,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
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
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            epoch: EpochId::start(),
        }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: usize) {
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

unsafe impl<T> PhantomQuery for QueryBorrowAny<&'static mut T>
where
    T: Send + ?Sized + 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchBorrowAnyWrite<'a, T>;

    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.contains_borrow_mut(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn access_archetype(archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        for (id, _) in archetype
            .borrow_mut_indices(TypeId::of::<T>())
            .unwrap_unchecked()
        {
            f(*id, Access::Write);
        }
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, epoch: EpochId) -> FetchBorrowAnyWrite<T> {
        let (id, idx) = *archetype
            .borrow_mut_indices(TypeId::of::<T>())
            .unwrap_unchecked()
            .get_unchecked(0);

        let component = archetype.component(id).unwrap_unchecked();
        debug_assert_eq!(component.borrows()[idx].target(), TypeId::of::<T>());

        let data = component.data_mut();

        FetchBorrowAnyWrite {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: component.borrows()[idx].borrow_mut().unwrap_unchecked(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            epoch,
        }
    }
}
