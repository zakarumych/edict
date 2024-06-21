use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::{BorrowFn, BorrowFnMut, ComponentInfo},
    epoch::EpochId,
    query::{
        read::Read, write::Write, Access, AsQuery, Fetch, ImmutableQuery, IntoQuery, Query,
        SendQuery, WriteAlias,
    },
    type_id,
};

/// [`Query`] that fetches components with specific `TypeId` as specified borrow.
pub struct BorrowOne<T> {
    ty: TypeId,
    marker: PhantomData<T>,
}

impl<T> Copy for BorrowOne<T> {}

impl<T> Clone for BorrowOne<T> {
    fn clone(&self) -> Self {
        *self
    }

    fn clone_from(&mut self, source: &Self) {
        self.ty = source.ty;
    }
}

impl<T> BorrowOne<T> {
    /// Construct a new query that fetches component with specified id.
    /// Borrowing it as `T`.
    pub fn new(ty: TypeId) -> Self {
        BorrowOne {
            ty,
            marker: PhantomData,
        }
    }
}

/// [`Fetch`] for [`BorrowOne<&T>`].
pub struct FetchBorrowOneRead<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: BorrowFn<T>,
    marker: PhantomData<&'a T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowOneRead<'a, T>
where
    T: ?Sized + 'a,
{
    type Item = &'a T;

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowOneRead {
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

impl<T> AsQuery for BorrowOne<&T>
where
    T: ?Sized + 'static,
{
    type Query = BorrowOne<Read<T>>;
}

impl<T> AsQuery for BorrowOne<Read<T>>
where
    T: ?Sized + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for BorrowOne<Read<T>>
where
    T: ?Sized + 'static,
{
    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for BorrowOne<Read<T>>
where
    T: ?Sized + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchBorrowOneRead<'a, T>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == self.ty {
            assert!(
                comp.has_borrow(type_id::<T>()),
                "Component does not have the borrow"
            );

            Ok(Some(Access::Read))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(self.ty)
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(self.ty, Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchBorrowOneRead<'a, T> {
        let component = unsafe { archetype.component(self.ty).unwrap_unchecked() };

        let cb = component
            .borrows()
            .iter()
            .find(|&cb| cb.target() == type_id::<T>())
            .unwrap();

        let data = unsafe { component.data() };

        FetchBorrowOneRead {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: cb.borrow(),
            marker: PhantomData::<&'a T>,
        }
    }
}

unsafe impl<T> ImmutableQuery for BorrowOne<Read<T>> where T: ?Sized + 'static {}
unsafe impl<T> SendQuery for BorrowOne<Read<T>> where T: Sync + ?Sized + 'static {}

/// [`Fetch`] for [`BorrowOne<&mut T>`].
pub struct FetchBorrowOneWrite<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: BorrowFnMut<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    marker: PhantomData<&'a mut T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowOneWrite<'a, T>
where
    T: ?Sized + 'a,
{
    type Item = &'a mut T;

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowOneWrite {
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
        let chunk_version = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_version.bump(self.epoch);
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> &'a mut T {
        let entity_version = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
        entity_version.bump(self.epoch);

        unsafe {
            (self.borrow_fn)(
                NonNull::new_unchecked(self.ptr.as_ptr().add(idx as usize * self.size)),
                self.marker,
            )
        }
    }
}

impl<T> AsQuery for BorrowOne<&mut T>
where
    T: ?Sized + 'static,
{
    type Query = BorrowOne<Write<T>>;
}

impl<T> AsQuery for BorrowOne<Write<T>>
where
    T: ?Sized + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for BorrowOne<Write<T>>
where
    T: ?Sized + 'static,
{
    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<T> Query for BorrowOne<Write<T>>
where
    T: ?Sized + 'static,
{
    type Item<'a> = &'a mut T;
    type Fetch<'a> = FetchBorrowOneWrite<'a, T>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == self.ty {
            assert!(
                comp.has_borrow_mut(type_id::<T>()),
                "Component does not have the borrow_mut"
            );

            Ok(Some(Access::Write))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(self.ty)
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(self.ty, Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchBorrowOneWrite<'a, T> {
        let component = unsafe { archetype.component(self.ty).unwrap_unchecked() };

        let cb = component
            .borrows()
            .iter()
            .find(|&cb| cb.target() == type_id::<T>())
            .unwrap();

        assert!(cb.borrow_mut::<T>().is_some());

        let data = unsafe { component.data_mut() };

        data.epoch.bump(epoch);

        FetchBorrowOneWrite {
            ptr: data.ptr,
            size: component.layout().size(),
            borrow_fn: unsafe { cb.borrow_mut().unwrap_unchecked() },
            entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
            chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
            epoch,
            marker: PhantomData::<&mut T>,
        }
    }
}

unsafe impl<T> SendQuery for BorrowOne<Write<T>> where T: Send + ?Sized + 'static {}
