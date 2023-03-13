use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{
    phantom::PhantomQuery, Access, Fetch, ImmutablePhantomQuery, ImmutableQuery, IntoQuery,
};

/// [`Fetch`] type for the `&T` query.

pub struct FetchCopied<'a, T> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchCopied<'a, T>
where
    T: Copy + Sync + 'a,
{
    type Item = T;

    #[inline]
    fn dangling() -> Self {
        FetchCopied {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> T {
        *self.ptr.as_ptr().add(idx)
    }
}

/// Query to yield copies of specified component.
///
/// Skips entities that don't have the component.
pub struct Copied<T>(PhantomData<T>);

impl<T> Copied<T>
where
    T: Copy + Sync + 'static,
{
    /// Creates a new [`Copied`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

impl<T> IntoQuery for Copied<T>
where
    T: Copy + Sync + 'static,
{
    type Query = PhantomData<fn() -> Self>;
}

unsafe impl<T> PhantomQuery for Copied<T>
where
    T: Copy + Sync + 'static,
{
    type Item<'a> = T;
    type Fetch<'a> = FetchCopied<'a, T>;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: EpochId) -> FetchCopied<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data();

        FetchCopied {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for Copied<T> where T: Copy + Sync + 'static {}

/// Returns query that yields copies of specified component
/// for each entity that has that component.
///
/// Skips entities that don't have the component.
pub fn copied<T>() -> PhantomData<fn() -> Copied<T>>
where
    T: Sync,
    for<'a> PhantomData<fn() -> Copied<T>>: ImmutableQuery,
{
    PhantomData
}
