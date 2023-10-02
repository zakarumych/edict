use core::any::TypeId;

use crate::{archetype::Archetype, epoch::EpochId, system::QueryArg};

use super::{Access, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, WriteAlias};

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

impl<T> DefaultQuery for Option<T>
where
    T: DefaultQuery,
{
    #[inline(always)]
    fn default_query() -> Self::Query {
        Some(T::default_query())
    }
}

impl<T> QueryArg for Option<T>
where
    T: QueryArg,
{
    #[inline(always)]
    fn new() -> Self::Query {
        Some(T::new())
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
    fn component_type_access(&self, ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        match self {
            None => Ok(None),
            Some(t) => t.component_type_access(ty),
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, _: &Archetype) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        match self {
            Some(t) if t.visit_archetype(archetype) => t.access_archetype(archetype, f),
            _ => {}
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
            Some(t) if t.visit_archetype(archetype) => Some(t.fetch(arch_idx, archetype, epoch)),
            _ => None,
        }
    }
}

unsafe impl<T> ImmutableQuery for Option<T> where T: ImmutableQuery {}
