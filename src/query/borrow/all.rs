use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use alloc::vec::Vec;
use atomicell::borrow::AtomicBorrow;

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, IntoQuery, PhantomQuery, PhantomQueryFetch},
};

/// [`PhantomQuery`] that borrows from components.
pub struct QueryBorrowAll<T> {
    marker: PhantomData<fn() -> T>,
}

struct FetchBorrowAllReadComponent<'a, T: ?Sized> {
    ptr: NonNull<u8>,
    size: usize,
    borrow_fn: unsafe fn(NonNull<u8>, PhantomData<&'a ()>) -> &'a T,
    _borrow: AtomicBorrow<'a>,
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

    #[inline]
    fn dangling() -> Self {
        FetchBorrowAllRead {
            components: Vec::new(),
            marker: PhantomData,
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
    unsafe fn get_item(&mut self, idx: usize) -> Vec<&'a T> {
        self.components
            .iter()
            .map(|c| {
                (c.borrow_fn)(
                    NonNull::new_unchecked(c.ptr.as_ptr().add(idx * c.size)),
                    PhantomData::<&'a ()>,
                )
            })
            .collect()
    }
}

impl<'a, T> PhantomQueryFetch<'a> for QueryBorrowAll<&T>
where
    T: Sync + ?Sized + 'a,
{
    type Item = Vec<&'a T>;
    type Fetch = FetchBorrowAllRead<'a, T>;
}

impl<T> IntoQuery for QueryBorrowAll<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Query = PhantomData<fn() -> Self>;
}

impl<T> PhantomQuery for QueryBorrowAll<&T>
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
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: EpochId) -> FetchBorrowAllRead<'a, T> {
        let components = archetype
            .borrow_indices(TypeId::of::<T>())
            .unwrap_unchecked()
            .iter()
            .map(|&(cidx, bidx)| {
                let component = archetype.component(cidx);
                debug_assert_eq!(component.borrows()[bidx].target(), TypeId::of::<T>());

                let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

                FetchBorrowAllReadComponent {
                    ptr: data.ptr,
                    size: component.layout().size(),
                    borrow_fn: component.borrows()[bidx].borrow(),
                    _borrow: borrow,
                }
            })
            .collect();

        FetchBorrowAllRead {
            components,
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for QueryBorrowAll<&T> where T: Sync + ?Sized + 'static {}
