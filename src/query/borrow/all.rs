use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use alloc::vec::Vec;

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, IntoQuery, PhantomQuery},
};

phantom_newtype! {
    /// [`PhantomQuery`] that borrows from components.
    pub struct QueryBorrowAll<T>
}

impl<T> QueryBorrowAll<&T>
where
    T: Sync + ?Sized + 'static,
{
    /// Creates a new [`QueryBorrowAll`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
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

    #[inline]
    fn dangling() -> Self {
        FetchBorrowAllRead {
            components: Vec::new(),
            marker: PhantomData,
        }
    }

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

impl<T> IntoQuery for QueryBorrowAll<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Query = PhantomData<fn() -> Self>;

    #[inline]
    fn into_query(self) -> Self::Query {
        PhantomData
    }
}

unsafe impl<T> PhantomQuery for QueryBorrowAll<&T>
where
    T: Sync + ?Sized + 'static,
{
    type Item<'a> = Vec<&'a T>;
    type Fetch<'a> = FetchBorrowAllRead<'a, T>;

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
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: EpochId) -> FetchBorrowAllRead<'a, T> {
        let components = archetype
            .borrow_indices(TypeId::of::<T>())
            .unwrap_unchecked()
            .iter()
            .map(|&(id, idx)| {
                let component = archetype.component(id).unwrap_unchecked();
                debug_assert_eq!(component.borrows()[idx].target(), TypeId::of::<T>());

                let data = component.data();

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

unsafe impl<T> ImmutablePhantomQuery for QueryBorrowAll<&T> where T: Sync + ?Sized + 'static {}
