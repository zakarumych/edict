use alloc::vec::Vec;
use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{
        read::Read, Access, AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query,
        SendQuery, Write, WriteAlias,
    },
    system::QueryArg,
};

/// Query that borrows from components.
#[derive(Clone, Copy, Debug, Default)]
pub struct QueryBorrowAll<T>(pub T);

struct FetchBorrowAllReadComponent<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
}

/// [`Fetch`] for [`QueryBorrowAll<&T>`].
pub struct FetchBorrowAllRead<'a, T: ?Sized> {
    components: Vec<FetchBorrowAllReadComponent<'a, T>>,
    marker: PhantomData<fn() -> T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAllRead<'a, T>
where
    T: ?Sized + 'a,
{
    type Item = Vec<&'a T>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowAllRead {
            components: Vec::new(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> Vec<&'a T> {
        self.components
            .iter()
            .map(|c| unsafe {
                (c.borrow_fn)(
                    NonNull::new_unchecked(c.ptr.as_ptr().add(idx as usize * c.size)),
                    PhantomData::<&'a ()>,
                )
            })
            .collect()
    }
}

impl<T> AsQuery for QueryBorrowAll<&T>
where
    T: ?Sized + 'static,
{
    type Query = QueryBorrowAll<Read<T>>;
}

impl<T> DefaultQuery for QueryBorrowAll<&T>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> QueryBorrowAll<Read<T>> {
        QueryBorrowAll(Read)
    }
}

impl<T> AsQuery for QueryBorrowAll<Read<T>>
where
    T: ?Sized + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for QueryBorrowAll<Read<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for QueryBorrowAll<Read<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        QueryBorrowAll(Read)
    }
}

impl<T> QueryArg for QueryBorrowAll<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        QueryBorrowAll(Read)
    }
}

unsafe impl<T> Query for QueryBorrowAll<Read<T>>
where
    T: ?Sized + 'static,
{
    type Item<'a> = Vec<&'a T>;
    type Fetch<'a> = FetchBorrowAllRead<'a, T>;

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
        let indices = unsafe {
            archetype
                .borrow_indices(TypeId::of::<T>())
                .unwrap_unchecked()
        };
        for (id, _) in indices {
            f(*id, Access::Read);
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchBorrowAllRead<'a, T> {
        let indices = unsafe {
            archetype
                .borrow_indices(TypeId::of::<T>())
                .unwrap_unchecked()
        };
        let components = indices
            .iter()
            .map(|&(id, idx)| {
                let component = unsafe { archetype.component(id).unwrap_unchecked() };
                debug_assert_eq!(component.borrows()[idx].target(), TypeId::of::<T>());

                let data = unsafe { component.data() };

                FetchBorrowAllReadComponent {
                    ptr: data.ptr,
                    size: component.layout().size(),
                    borrow_fn: component.borrows()[idx].borrow(),
                }
            })
            .collect();

        FetchBorrowAllRead {
            components,
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for QueryBorrowAll<Read<T>> where T: ?Sized + 'static {}
unsafe impl<T> SendQuery for QueryBorrowAll<Read<T>> where T: Sync + ?Sized + 'static {}

struct FetchBorrowAllWriteComponent<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a mut ()>) -> &'a mut T,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
}

/// [`Fetch`] for [`QueryBorrowAll<&mut T>`].
pub struct FetchBorrowAllWrite<'a, T: ?Sized> {
    components: Vec<FetchBorrowAllWriteComponent<'a, T>>,
    epoch: EpochId,
    marker: PhantomData<fn() -> T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAllWrite<'a, T>
where
    T: ?Sized + 'a,
{
    type Item = Vec<&'a mut T>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowAllWrite {
            components: Vec::new(),
            epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        self.components.iter_mut().for_each(|c| {
            let chunk_epoch = &mut *c.chunk_epochs.as_ptr().add(chunk_idx as usize);
            chunk_epoch.bump(self.epoch);
        })
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> Vec<&'a mut T> {
        self.components
            .iter_mut()
            .map(|c| unsafe {
                let entity_version = &mut *c.entity_epochs.as_ptr().add(idx as usize);
                entity_version.bump(self.epoch);

                (c.borrow_fn)(
                    NonNull::new_unchecked(c.ptr.as_ptr().add(idx as usize * c.size)),
                    PhantomData::<&'a mut ()>,
                )
            })
            .collect()
    }
}

impl<T> AsQuery for QueryBorrowAll<&mut T>
where
    T: ?Sized + 'static,
{
    type Query = QueryBorrowAll<Write<T>>;
}

impl<T> DefaultQuery for QueryBorrowAll<&mut T>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> QueryBorrowAll<Write<T>> {
        QueryBorrowAll(Write)
    }
}

impl<T> AsQuery for QueryBorrowAll<Write<T>>
where
    T: ?Sized + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for QueryBorrowAll<Write<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for QueryBorrowAll<Write<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        QueryBorrowAll(Write)
    }
}

impl<T> QueryArg for QueryBorrowAll<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        QueryBorrowAll(Write)
    }
}

unsafe impl<T> Query for QueryBorrowAll<Write<T>>
where
    T: ?Sized + 'static,
{
    type Item<'a> = Vec<&'a mut T>;
    type Fetch<'a> = FetchBorrowAllWrite<'a, T>;

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
        let indices = unsafe {
            archetype
                .borrow_mut_indices(TypeId::of::<T>())
                .unwrap_unchecked()
        };
        for (id, _) in indices {
            f(*id, Access::Write);
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchBorrowAllWrite<'a, T> {
        let indices = unsafe {
            archetype
                .borrow_indices(TypeId::of::<T>())
                .unwrap_unchecked()
        };
        let components = indices
            .iter()
            .map(|&(id, idx)| {
                let component = unsafe { archetype.component(id).unwrap_unchecked() };
                debug_assert_eq!(component.borrows()[idx].target(), TypeId::of::<T>());

                let data = unsafe { component.data_mut() };
                data.epoch.bump(epoch);

                FetchBorrowAllWriteComponent {
                    ptr: data.ptr,
                    size: component.layout().size(),
                    borrow_fn: component.borrows()[idx].borrow_mut().unwrap(),
                    entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
                    chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
                }
            })
            .collect();

        FetchBorrowAllWrite {
            components,
            epoch,
            marker: PhantomData,
        }
    }
}

unsafe impl<T> SendQuery for QueryBorrowAll<Write<T>> where T: Send + ?Sized + 'static {}
