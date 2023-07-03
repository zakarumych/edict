use core::any::TypeId;

use crate::{archetype::Archetype, epoch::EpochId};

use super::{Access, Fetch, ImmutableQuery, IntoQuery, Query};

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
    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        if let Some(fetch) = self {
            fetch.visit_chunk(chunk_idx)
        } else {
            true
        }
    }

    /// Notifies this fetch that it visits a chunk.
    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        if let Some(fetch) = self {
            fetch.touch_chunk(chunk_idx);
        }
    }

    /// Checks if item with specified index must be visited or skipped.
    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        if let Some(fetch) = self {
            fetch.visit_item(idx)
        } else {
            true
        }
    }

    /// Returns fetched item at specified index.
    unsafe fn get_item(&mut self, idx: u32) -> Option<T::Item> {
        match self {
            None => None,
            Some(fetch) => Some(fetch.get_item(idx)),
        }
    }
}

impl<T> IntoQuery for Option<T>
where
    T: IntoQuery,
{
    type Query = Option<T::Query>;

    #[inline(always)]
    fn into_query(self) -> Self::Query {
        self.map(IntoQuery::into_query)
    }
}

unsafe impl<T> Query for Option<T>
where
    T: Query,
{
    type Item<'a> = Option<T::Item<'a>>;
    type Fetch<'a> = Option<T::Fetch<'a>>;

    const MUTABLE: bool = T::MUTABLE;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        match self {
            None => None,
            Some(t) => t.access(ty),
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, _: &Archetype) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        if let Some(t) = self {
            if t.visit_archetype(archetype) {
                t.access_archetype(archetype, f)
            }
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Option<T::Fetch<'a>> {
        match self {
            None => None,
            Some(t) => Some(t.fetch(arch_idx, archetype, epoch)),
        }
    }
}

unsafe impl<T> ImmutableQuery for Option<T> where T: ImmutableQuery {}
