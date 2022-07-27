use core::any::TypeId;

use crate::archetype::Archetype;

use super::{Access, Fetch, ImmutablePhantomQuery, PhantomQuery, Query};

unsafe impl<'a, T> Fetch<'a> for Option<T>
where
    T: Fetch<'a>,
{
    type Item = Option<T::Item>;

    /// Returns `Fetch` value that must not be used.
    fn dangling() -> Self {
        None
    }

    /// Checks if chunk with specified index must be skipped.
    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        if let Some(fetch) = self {
            fetch.skip_chunk(chunk_idx)
        } else {
            false
        }
    }

    /// Notifies this fetch that it visits a chunk.
    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        if let Some(fetch) = self {
            fetch.visit_chunk(chunk_idx);
        }
    }

    /// Checks if item with specified index must be skipped.
    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        if let Some(fetch) = self {
            fetch.skip_item(idx)
        } else {
            false
        }
    }

    /// Returns fetched item at specifeid index.
    unsafe fn get_item(&mut self, idx: usize) -> Option<T::Item> {
        match self {
            None => None,
            Some(fetch) => Some(fetch.get_item(idx)),
        }
    }
}

unsafe impl<T> PhantomQuery for Option<T>
where
    T: PhantomQuery,
{
    type Fetch = Option<T::Fetch>;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        T::access(ty)
    }

    #[inline]
    fn conflicts<Q>(other: &Q) -> bool
    where
        Q: Query,
    {
        T::conflicts(other)
    }

    #[inline]
    fn is_valid() -> bool {
        T::is_valid()
    }

    #[inline]
    fn skip_archetype(_: &Archetype) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, epoch: u64) -> Option<T::Fetch> {
        if T::skip_archetype(archetype) {
            None
        } else {
            Some(T::fetch(archetype, epoch))
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for Option<T> where T: ImmutablePhantomQuery {}
