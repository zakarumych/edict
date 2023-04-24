use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{assert_immutable_query, phantom::PhantomQuery, Access, Fetch, ImmutablePhantomQuery};

/// [`Fetch`] type for the `&T` query.

pub struct FetchRead<'a, T> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchRead<'a, T>
where
    T: Sync + 'a,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        FetchRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a T {
        &*self.ptr.as_ptr().add(idx)
    }
}

unsafe impl<T> PhantomQuery for &T
where
    T: Sync + 'static,
{
    type Item<'a> = &'a T;
    type Fetch<'a> = FetchRead<'a, T>;

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
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: EpochId) -> FetchRead<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data();

        FetchRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for &T where T: Sync + 'static {}

/// [`Query`] type for the `&T` phantom query.
pub type Read<T> = PhantomData<fn() -> &'static T>;

/// Returns query that yields reference to specified component
/// for each entity that has that component.
///
/// Skips entities that don't have the component.
pub fn read<T>() -> Read<T>
where
    T: Sync,
{
    assert_immutable_query::<Read<T>>();

    PhantomData
}
