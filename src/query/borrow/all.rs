use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use alloc::vec::Vec;

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{read::Read, Access, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query},
};

marker_type! {
    /// [`PhantomQuery`] that borrows from components.
    pub struct QueryBorrowAll<T>;
}

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
    T: Sync + ?Sized + 'a,
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

impl<T> IntoQuery for QueryBorrowAll<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Query = QueryBorrowAll<Read<T>>;

    #[inline(always)]
    fn into_query(self) -> QueryBorrowAll<Read<T>> {
        QueryBorrowAll
    }
}

impl<T> DefaultQuery for QueryBorrowAll<&T>
where
    T: Sync + ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> QueryBorrowAll<Read<T>> {
        QueryBorrowAll
    }
}

impl<T> IntoQuery for QueryBorrowAll<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for QueryBorrowAll<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        QueryBorrowAll::new()
    }
}

unsafe impl<T> Query for QueryBorrowAll<Read<T>>
where
    T: Sync + ?Sized + 'static,
{
    type Item<'a> = Vec<&'a T>;
    type Fetch<'a> = FetchBorrowAllRead<'a, T>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn access(&self, _ty: TypeId) -> Option<Access> {
        Some(Access::Read)
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

unsafe impl<T> ImmutableQuery for QueryBorrowAll<Read<T>> where T: Sync + ?Sized + 'static {}
