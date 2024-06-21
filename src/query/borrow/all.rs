use alloc::rc::Rc;
use core::{any::TypeId, fmt::Debug, iter::FusedIterator, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::{BorrowFn, BorrowFnMut, ComponentInfo},
    epoch::EpochId,
    query::{
        read::Read, Access, AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query,
        SendQuery, Write, WriteAlias,
    },
    system::QueryArg,
    type_id,
};

/// Query that borrows from components.
#[derive(Clone, Copy, Debug, Default)]
pub struct BorrowAll<T>(pub T);

struct FetchBorrowAllComponent<T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: BorrowFn<T>,
    borrow_mut_fn: Option<BorrowFnMut<T>>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
}

impl<T> Clone for FetchBorrowAllComponent<T>
where
    T: ?Sized,
{
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for FetchBorrowAllComponent<T> where T: ?Sized {}

pub struct BorrowAllRead<'a, T: ?Sized> {
    idx: u32,
    comp_idx: usize,
    components: Rc<[FetchBorrowAllComponent<T>]>,
    marker: PhantomData<&'a T>,
}

impl<'a, T> Clone for BorrowAllRead<'a, T>
where
    T: ?Sized,
{
    #[inline(always)]
    fn clone(&self) -> Self {
        BorrowAllRead {
            idx: self.idx,
            comp_idx: self.comp_idx,
            components: self.components.clone(),
            marker: self.marker,
        }
    }

    #[inline(always)]
    fn clone_from(&mut self, source: &Self) {
        self.idx = source.idx;
        self.comp_idx = source.comp_idx;
        self.components = source.components.clone();
    }
}

impl<'a, T> Debug for BorrowAllRead<'a, T>
where
    T: Debug + ?Sized,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.clone()).finish()
    }
}

impl<'a, T> Iterator for BorrowAllRead<'a, T>
where
    T: ?Sized,
{
    type Item = &'a T;

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    #[inline(always)]
    fn next(&mut self) -> Option<&'a T> {
        let c = &self.components.get(self.comp_idx)?;
        let r = unsafe {
            (c.borrow_fn)(
                NonNull::new_unchecked(c.ptr.as_ptr().add(self.idx as usize * c.size)),
                self.marker,
            )
        };
        self.comp_idx += 1;
        Some(r)
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<&'a T> {
        if n >= self.components.len() - self.comp_idx {
            self.comp_idx = self.components.len();
        } else {
            self.comp_idx += n;
        }
        self.next()
    }

    fn fold<B, F>(mut self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, &'a T) -> B,
    {
        let mut accum = init;
        for comp_idx in self.comp_idx..self.components.len() {
            let c = &self.components[comp_idx];
            let r = unsafe {
                (c.borrow_fn)(
                    NonNull::new_unchecked(c.ptr.as_ptr().add(self.idx as usize * c.size)),
                    self.marker,
                )
            };
            self.comp_idx += 1;
            accum = f(accum, r);
        }
        accum
    }
}

impl<'a, T> ExactSizeIterator for BorrowAllRead<'a, T>
where
    T: ?Sized,
{
    #[inline(always)]
    fn len(&self) -> usize {
        self.components.len() - self.comp_idx
    }
}

impl<'a, T> FusedIterator for BorrowAllRead<'a, T> where T: ?Sized {}

/// [`Fetch`] for [`BorrowAll<&T>`].
pub struct FetchBorrowAllRead<'a, T: ?Sized> {
    components: Rc<[FetchBorrowAllComponent<T>]>,
    marker: PhantomData<&'a T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAllRead<'a, T>
where
    T: ?Sized + 'a,
{
    type Item = BorrowAllRead<'a, T>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowAllRead {
            components: Rc::new([]),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> BorrowAllRead<'a, T> {
        BorrowAllRead {
            idx,
            comp_idx: 0,
            components: self.components.clone(),
            marker: self.marker,
        }
    }
}

impl<T> AsQuery for BorrowAll<&T>
where
    T: ?Sized + 'static,
{
    type Query = BorrowAll<Read<T>>;
}

impl<T> DefaultQuery for BorrowAll<&T>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> BorrowAll<Read<T>> {
        BorrowAll(Read)
    }
}

impl<T> AsQuery for BorrowAll<Read<T>>
where
    T: ?Sized + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for BorrowAll<Read<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for BorrowAll<Read<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        BorrowAll(Read)
    }
}

impl<T> QueryArg for BorrowAll<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        BorrowAll(Read)
    }
}

unsafe impl<T> Query for BorrowAll<Read<T>>
where
    T: ?Sized + 'static,
{
    type Item<'a> = BorrowAllRead<'a, T>;
    type Fetch<'a> = FetchBorrowAllRead<'a, T>;

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
        let indices = unsafe { archetype.borrow_indices(type_id::<T>()).unwrap_unchecked() };
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
        let indices = unsafe { archetype.borrow_indices(type_id::<T>()).unwrap_unchecked() };
        let components = indices
            .iter()
            .map(|&(id, idx)| {
                let component = unsafe { archetype.component(id).unwrap_unchecked() };
                debug_assert_eq!(component.borrows()[idx].target(), type_id::<T>());

                let data = unsafe { component.data_mut() };

                FetchBorrowAllComponent {
                    ptr: data.ptr,
                    size: component.layout().size(),
                    borrow_fn: component.borrows()[idx].borrow(),
                    borrow_mut_fn: None,
                    entity_epochs: unsafe {
                        NonNull::new_unchecked(data.entity_epochs.as_mut_ptr())
                    },
                    chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
                }
            })
            .collect();

        FetchBorrowAllRead {
            components,
            marker: PhantomData::<&'a T>,
        }
    }
}

unsafe impl<T> ImmutableQuery for BorrowAll<Read<T>> where T: ?Sized + 'static {}
unsafe impl<T> SendQuery for BorrowAll<Read<T>> where T: Sync + ?Sized + 'static {}

pub struct BorrowAllWrite<'a, T: ?Sized> {
    idx: u32,
    epoch: EpochId,
    comp_idx: usize,
    components: Rc<[FetchBorrowAllComponent<T>]>,
    marker: PhantomData<&'a mut T>,
}

impl<'a, T> BorrowAllWrite<'a, T>
where
    T: ?Sized,
{
    /// Borrow write iterator as read iterator.
    ///
    /// Returned read iteartor will yield shared reference to the components
    /// and may be iterated multiple times.
    /// It won't yield components that were already yielded by this iterator.
    ///
    /// After all copies of returned read iterator are dropped, write iterator
    /// will be able to yield mutable reference to components that were yielded by read iterator.
    pub fn read(&self) -> BorrowAllRead<'_, T> {
        BorrowAllRead {
            idx: self.idx,
            comp_idx: self.comp_idx,
            components: self.components.clone(),
            marker: PhantomData::<&T>,
        }
    }

    /// Turn write iterator into read iterator.
    ///
    /// Returned read iteartor will yield shared reference to the components
    /// and may be iterated multiple times.
    /// It won't yield components that were already yielded by this iterator.
    pub fn into_read(self) -> BorrowAllRead<'a, T> {
        BorrowAllRead {
            idx: self.idx,
            comp_idx: self.comp_idx,
            components: self.components.clone(),
            marker: PhantomData::<&'a T>,
        }
    }
}

impl<T> Debug for BorrowAllWrite<'_, T>
where
    T: Debug + ?Sized,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.read()).finish()
    }
}

impl<'a, T> Iterator for BorrowAllWrite<'a, T>
where
    T: ?Sized,
{
    type Item = &'a mut T;

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    #[inline(always)]
    fn next(&mut self) -> Option<&'a mut T> {
        let c = &self.components.get(self.comp_idx)?;
        let r = unsafe {
            let entity_version = &mut *c.entity_epochs.as_ptr().add(self.idx as usize);
            entity_version.bump(self.epoch);

            // Safety: mutable borrow function exists. Checked in `Query::fetch`.
            (c.borrow_mut_fn.unwrap_unchecked())(
                NonNull::new_unchecked(c.ptr.as_ptr().add(self.idx as usize * c.size)),
                self.marker,
            )
        };
        self.comp_idx += 1;
        Some(r)
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<&'a mut T> {
        if n >= self.components.len() - self.comp_idx {
            self.comp_idx = self.components.len();
        } else {
            self.comp_idx += n;
        }
        self.next()
    }

    fn fold<B, F>(mut self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, &'a mut T) -> B,
    {
        let mut accum = init;
        for comp_idx in self.comp_idx..self.components.len() {
            let c = &self.components[comp_idx];

            let r = unsafe {
                let entity_version = &mut *c.entity_epochs.as_ptr().add(self.idx as usize);
                entity_version.bump(self.epoch);

                // Safety: mutable borrow function exists. Checked in `Query::fetch`.
                (c.borrow_mut_fn.unwrap_unchecked())(
                    NonNull::new_unchecked(c.ptr.as_ptr().add(self.idx as usize * c.size)),
                    self.marker,
                )
            };
            self.comp_idx += 1;
            accum = f(accum, r);
        }
        accum
    }
}

impl<'a, T> ExactSizeIterator for BorrowAllWrite<'a, T>
where
    T: ?Sized,
{
    #[inline(always)]
    fn len(&self) -> usize {
        self.components.len() - self.comp_idx
    }
}

impl<'a, T> FusedIterator for BorrowAllWrite<'a, T> where T: ?Sized {}

/// [`Fetch`] for [`BorrowAll<&mut T>`].
pub struct FetchBorrowAllWrite<'a, T: ?Sized> {
    components: Rc<[FetchBorrowAllComponent<T>]>,
    epoch: EpochId,
    marker: PhantomData<&'a mut T>,
}

unsafe impl<'a, T> Fetch<'a> for FetchBorrowAllWrite<'a, T>
where
    T: ?Sized + 'a,
{
    type Item = BorrowAllWrite<'a, T>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchBorrowAllWrite {
            components: Rc::new([]),
            epoch: EpochId::start(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        self.components.iter().for_each(|c| {
            let chunk_epoch = unsafe { &mut *c.chunk_epochs.as_ptr().add(chunk_idx as usize) };
            chunk_epoch.bump(self.epoch);
        })
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> BorrowAllWrite<'a, T> {
        BorrowAllWrite {
            idx,
            epoch: self.epoch,
            comp_idx: 0,
            components: self.components.clone(),
            marker: self.marker,
        }
    }
}

impl<T> AsQuery for BorrowAll<&mut T>
where
    T: ?Sized + 'static,
{
    type Query = BorrowAll<Write<T>>;
}

impl<T> DefaultQuery for BorrowAll<&mut T>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> BorrowAll<Write<T>> {
        BorrowAll(Write)
    }
}

impl<T> AsQuery for BorrowAll<Write<T>>
where
    T: ?Sized + 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for BorrowAll<Write<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for BorrowAll<Write<T>>
where
    T: ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        BorrowAll(Write)
    }
}

impl<T> QueryArg for BorrowAll<Write<T>>
where
    T: Send + ?Sized + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        BorrowAll(Write)
    }
}

unsafe impl<T> Query for BorrowAll<Write<T>>
where
    T: ?Sized + 'static,
{
    type Item<'a> = BorrowAllWrite<'a, T>;
    type Fetch<'a> = FetchBorrowAllWrite<'a, T>;

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
        let indices = unsafe {
            archetype
                .borrow_mut_indices(type_id::<T>())
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
                .borrow_mut_indices(type_id::<T>())
                .unwrap_unchecked()
        };
        let components = indices
            .iter()
            .map(|&(id, idx)| {
                let component = unsafe { archetype.component(id).unwrap_unchecked() };
                debug_assert_eq!(component.borrows()[idx].target(), type_id::<T>());
                debug_assert!(component.borrows()[idx].borrow_mut::<T>().is_some());

                let data = unsafe { component.data_mut() };
                data.epoch.bump(epoch);

                FetchBorrowAllComponent {
                    ptr: data.ptr,
                    size: component.layout().size(),
                    borrow_fn: component.borrows()[idx].borrow(),
                    borrow_mut_fn: component.borrows()[idx].borrow_mut(),
                    entity_epochs: unsafe {
                        NonNull::new_unchecked(data.entity_epochs.as_mut_ptr())
                    },
                    chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
                }
            })
            .collect();

        FetchBorrowAllWrite {
            components,
            epoch,
            marker: PhantomData::<&'a mut T>,
        }
    }
}

unsafe impl<T> SendQuery for BorrowAll<Write<T>> where T: Send + ?Sized + 'static {}
