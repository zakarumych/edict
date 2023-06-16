use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use alloc::vec::Vec;

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, PhantomQuery},
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

unsafe impl<T> PhantomQuery for QueryBorrowAll<&'static T>
where
    T: Sync + ?Sized + 'static,
{
    type Item<'a> = Vec<&'a T>;
    type Fetch<'a> = FetchBorrowAllRead<'a, T>;

    const MUTABLE: bool = false;

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
        let indices = unsafe {
            archetype
                .borrow_indices(TypeId::of::<T>())
                .unwrap_unchecked()
        };
        for (id, _) in indices {
            f(*id, Access::Read);
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
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

unsafe impl<T> ImmutablePhantomQuery for QueryBorrowAll<&'static T> where T: Sync + ?Sized + 'static {}
