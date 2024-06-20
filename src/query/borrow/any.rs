use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::{BorrowFn, BorrowFnMut, ComponentInfo},
    epoch::EpochId,
    query::{
        read::Read, write::Write, Access, AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery,
        Query, SendQuery, WriteAlias,
    },
    system::QueryArg,
    type_id,
};

/// Query that borrows from components.
#[derive(Clone, Copy, Debug, Default)]
pub struct BorrowAny<T>(pub T);

/// [`Fetch`] for [`BorrowAny<&T>`].
pub struct FetchBorrowAnyRead<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: BorrowFn<T>,
    marker: PhantomData<&'a T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAnyRead<'a, T>
where
    T: ?Sized + 'a,
{
    type Item = &'a T;

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowAnyRead {
            ptr: NonNull::dangling(),
            size: 0,
            borrow_fn: |_, _| unreachable!(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> &'a T {
        unsafe {
            (self.borrow_fn)(
                NonNull::new_unchecked(self.ptr.as_ptr().add(idx as usize * self.size)),
                self.marker,
            )
        }
    }
}

impl<T> AsQuery for BorrowAny<&T>
where
    T: ?Sized + 'static,
{
    type Query = BorrowAny<Read<T>>;
}

impl<T> DefaultQuery for BorrowAny<&T>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> BorrowAny<Read<T>> {
        BorrowAny(Read)
    }
}

impl<T> AsQuery for BorrowAny<Read<T>>
where
    T: ?Sized + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for BorrowAny<Read<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for BorrowAny<Read<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        BorrowAny(Read)
    }
}

impl<T> QueryArg for BorrowAny<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        BorrowAny(Read)
    }
}

unsafe impl<T> Query for BorrowAny<Read<T>>
where
    T: ?Sized + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchBorrowAnyRead<'a, T>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.has_borrow(type_id::<T>()) {
            Ok(Some(Access::Read))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.contains_borrow(type_id::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        for (id, _) in archetype.borrow_indices(type_id::<T>()).unwrap_unchecked() {
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
            .borrow_indices(type_id::<T>())
            .unwrap_unchecked()
            .get_unchecked(0);

        let component = archetype.component(id).unwrap_unchecked();
        debug_assert_eq!(component.borrows()[idx].target(), type_id::<T>());

        let data = component.data();

        FetchBorrowAnyRead {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: component.borrows()[idx].borrow(),
            marker: PhantomData::<&'a T>,
        }
    }
}

unsafe impl<T> ImmutableQuery for BorrowAny<Read<T>> where T: ?Sized + 'static {}
unsafe impl<T> SendQuery for BorrowAny<Read<T>> where T: Sync + ?Sized + 'static {}

/// [`Fetch`] for [`BorrowAny<&mut T>`].
pub struct FetchBorrowAnyWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: BorrowFnMut<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    marker: PhantomData<&'a mut T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAnyWrite<'a, T>
where
    T: ?Sized + 'static,
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
            marker: PhantomData,
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
            self.marker,
        )
    }
}

impl<T> AsQuery for BorrowAny<&mut T>
where
    T: ?Sized + 'static,
{
    type Query = BorrowAny<Write<T>>;
}

impl<T> DefaultQuery for BorrowAny<&mut T>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> BorrowAny<Write<T>> {
        BorrowAny(Write)
    }
}

impl<T> AsQuery for BorrowAny<Write<T>>
where
    T: ?Sized + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for BorrowAny<Write<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for BorrowAny<Write<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        BorrowAny(Write)
    }
}

impl<T> QueryArg for BorrowAny<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        BorrowAny(Write)
    }
}

unsafe impl<T> Query for BorrowAny<Write<T>>
where
    T: ?Sized + 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchBorrowAnyWrite<'a, T>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.has_borrow_mut(type_id::<T>()) {
            Ok(Some(Access::Write))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.contains_borrow_mut(type_id::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        for (id, _) in archetype
            .borrow_mut_indices(type_id::<T>())
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
            .borrow_mut_indices(type_id::<T>())
            .unwrap_unchecked()
            .get_unchecked(0);

        let component = archetype.component(id).unwrap_unchecked();
        debug_assert_eq!(component.borrows()[idx].target(), type_id::<T>());

        let data = unsafe { component.data_mut() };
        data.epoch.bump(epoch);

        FetchBorrowAnyWrite {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: component.borrows()[idx].borrow_mut().unwrap_unchecked(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            epoch,
            marker: PhantomData::<&'a mut T>,
        }
    }
}

unsafe impl<T> SendQuery for BorrowAny<Write<T>> where T: Send + ?Sized + 'static {}
