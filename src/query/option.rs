use core::{any::TypeId, marker::PhantomData};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{Access, Fetch, ImmutablePhantomQuery, IntoQuery, PhantomQuery};

unsafe impl<'a, T> Fetch<'a> for Option<T>
where
    T: Fetch<'a>,
{
    type Item = Option<T::Item>;

    /// Returns `Fetch` value that must not be used.
    fn dangling() -> Self {
        None
    }

    /// Checks if chunk with specified index must be visited or skipped.
    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) -> bool {
        if let Some(fetch) = self {
            fetch.visit_chunk(chunk_idx)
        } else {
            true
        }
    }

    /// Notifies this fetch that it visits a chunk.
    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: usize) {
        if let Some(fetch) = self {
            fetch.touch_chunk(chunk_idx);
        }
    }

    /// Checks if item with specified index must be visited or skipped.
    #[inline]
    unsafe fn visit_item(&mut self, idx: usize) -> bool {
        if let Some(fetch) = self {
            fetch.visit_item(idx)
        } else {
            true
        }
    }

    /// Returns fetched item at specified index.
    unsafe fn get_item(&mut self, idx: usize) -> Option<T::Item> {
        match self {
            None => None,
            Some(fetch) => Some(fetch.get_item(idx)),
        }
    }
}

impl<T> IntoQuery for Option<T>
where
    T: PhantomQuery,
{
    type Query = PhantomData<fn() -> Option<T>>;

    fn into_query(self) -> Self::Query {
        PhantomData
    }
}

unsafe impl<T> PhantomQuery for Option<T>
where
    T: PhantomQuery,
{
    type Item<'a> = Option<T::Item<'a>>;
    type Fetch<'a> = Option<T::Fetch<'a>>;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        T::access(ty)
    }

    #[inline]
    fn visit_archetype(_: &Archetype) -> bool {
        true
    }

    #[inline]
    unsafe fn access_archetype(archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        if T::visit_archetype(archetype) {
            T::access_archetype(archetype, f)
        }
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: EpochId) -> Option<T::Fetch<'a>> {
        if !T::visit_archetype(archetype) {
            None
        } else {
            Some(T::fetch(archetype, epoch))
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for Option<T> where T: ImmutablePhantomQuery {}
