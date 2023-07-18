use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{
        read::Read, write::Write, Access, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query,
        WriteAlias,
    },
    system::QueryArg,
};

marker_type! {
    /// [`PhantomQuery`] that borrows from components.
    pub struct QueryBorrowAny<T>;
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

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowAnyRead {
            ptr: NonNull::dangling(),
            size: 0,
            borrow_fn: |_, _| unreachable!(),
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> &'a T {
        unsafe {
            (self.borrow_fn)(
                NonNull::new_unchecked(self.ptr.as_ptr().add(idx as usize * self.size)),
                PhantomData::<&'a ()>,
            )
        }
    }
}

impl<T> IntoQuery for QueryBorrowAny<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Query = QueryBorrowAny<Read<T>>;

    #[inline(always)]
    fn into_query(self) -> QueryBorrowAny<Read<T>> {
        QueryBorrowAny
    }
}

impl<T> DefaultQuery for QueryBorrowAny<&T>
where
    T: Sync + ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> QueryBorrowAny<Read<T>> {
        QueryBorrowAny
    }
}

impl<T> IntoQuery for QueryBorrowAny<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for QueryBorrowAny<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        QueryBorrowAny::new()
    }
}

impl<T> QueryArg for QueryBorrowAny<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        QueryBorrowAny
    }
}

unsafe impl<T> Query for QueryBorrowAny<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchBorrowAnyRead<'a, T>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_type_access(&self, _ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(Some(Access::Read))
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.contains_borrow(TypeId::of::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        for (id, _) in archetype
            .borrow_indices(TypeId::of::<T>())
            .unwrap_unchecked()
        {
            f(*id, Access::Read);
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchBorrowAnyRead<'a, T> {
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

unsafe impl<T> ImmutableQuery for QueryBorrowAny<Read<T>> where T: Sync + ?Sized + 'static {}

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

    #[inline(always)]
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

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize);
        chunk_epoch.bump(self.epoch);
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

impl<T> IntoQuery for QueryBorrowAny<&mut T>
where
    T: Send + ?Sized + 'static,
{
    type Query = QueryBorrowAny<Write<T>>;

    #[inline(always)]
    fn into_query(self) -> QueryBorrowAny<Write<T>> {
        QueryBorrowAny
    }
}

impl<T> DefaultQuery for QueryBorrowAny<&mut T>
where
    T: Send + ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> QueryBorrowAny<Write<T>> {
        QueryBorrowAny
    }
}

impl<T> IntoQuery for QueryBorrowAny<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for QueryBorrowAny<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        QueryBorrowAny::new()
    }
}

impl<T> QueryArg for QueryBorrowAny<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        QueryBorrowAny
    }
}

unsafe impl<T> Query for QueryBorrowAny<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchBorrowAnyWrite<'a, T>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn component_type_access(&self, _ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(Some(Access::Write))
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.contains_borrow_mut(TypeId::of::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        for (id, _) in archetype
            .borrow_mut_indices(TypeId::of::<T>())
            .unwrap_unchecked()
        {
            f(*id, Access::Write);
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchBorrowAnyWrite<'a, T> {
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
