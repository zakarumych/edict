use core::any::TypeId;

use crate::archetype::Archetype;

use super::{
    Access, Fetch, ImmutablePhantomQuery, ImmutableQuery, PhantomQuery, PhantomQueryFetch, Query,
    QueryFetch,
};

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

impl<'a, T> PhantomQueryFetch<'a> for Option<T>
where
    T: PhantomQuery,
{
    type Item = Option<<T as PhantomQueryFetch<'a>>::Item>;
    type Fetch = Option<<T as PhantomQueryFetch<'a>>::Fetch>;
}

unsafe impl<T> PhantomQuery for Option<T>
where
    T: PhantomQuery,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        T::access(ty)
    }

    #[inline]
    fn access_any() -> Option<Access> {
        T::access_any()
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
    unsafe fn fetch<'a>(
        archetype: &'a Archetype,
        epoch: u64,
    ) -> Option<<T as PhantomQueryFetch<'a>>::Fetch> {
        if T::skip_archetype(archetype) {
            None
        } else {
            Some(T::fetch(archetype, epoch))
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for Option<T> where T: ImmutablePhantomQuery {}

impl<'a, T> QueryFetch<'a> for Option<T>
where
    T: Query,
{
    type Item = Option<<T as QueryFetch<'a>>::Item>;
    type Fetch = Option<<T as QueryFetch<'a>>::Fetch>;
}

unsafe impl<T> Query for Option<T>
where
    T: Query,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        self.as_ref()?.access(ty)
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        self.as_ref()?.access_any()
    }

    #[inline]
    fn conflicts<Q>(&self, other: &Q) -> bool
    where
        Q: Query,
    {
        match self {
            None => false,
            Some(query) => query.conflicts(other),
        }
    }

    #[inline]
    fn is_valid(&self) -> bool {
        match self {
            None => true,
            Some(query) => query.is_valid(),
        }
    }

    #[inline]
    fn skip_archetype(&self, _: &Archetype) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: u64,
    ) -> Option<<T as QueryFetch<'a>>::Fetch> {
        match self {
            None => None,
            Some(query) => match query.skip_archetype(archetype) {
                false => Some(query.fetch(archetype, epoch)),
                true => None,
            },
        }
    }
}

unsafe impl<T> ImmutableQuery for Option<T> where T: ImmutableQuery {}
