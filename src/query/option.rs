use core::any::TypeId;

use crate::{
    archetype::Archetype, component::ComponentInfo, entity::EntityId, epoch::EpochId,
    system::QueryArg,
};

use super::{
    Access, AsQuery, BatchFetch, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery,
    WriteAlias,
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

    /// Checks if chunk with specified index must be visited or skipped.
    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        if let Some(fetch) = self {
            unsafe { fetch.visit_chunk(chunk_idx) }
        } else {
            true
        }
    }

    /// Notifies this fetch that it visits a chunk.
    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        if let Some(fetch) = self {
            unsafe {
                fetch.touch_chunk(chunk_idx);
            }
        }
    }

    /// Checks if item with specified index must be visited or skipped.
    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        if let Some(fetch) = self {
            unsafe { fetch.visit_item(idx) }
        } else {
            true
        }
    }

    /// Returns fetched item at specified index.
    unsafe fn get_item(&mut self, idx: u32) -> Option<T::Item> {
        match self {
            None => None,
            Some(fetch) => Some(unsafe { fetch.get_item(idx) }),
        }
    }
}

unsafe impl<'a, T> BatchFetch<'a> for Option<T>
where
    T: BatchFetch<'a>,
{
    type Batch = Option<T::Batch>;

    /// Returns fetched item at specified index.
    unsafe fn get_batch(&mut self, start: u32, end: u32) -> Option<T::Batch> {
        match self {
            None => None,
            Some(fetch) => Some(unsafe { fetch.get_batch(start, end) }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OptionQuery<T>(pub T);

impl<T> AsQuery for Option<T>
where
    T: AsQuery,
{
    type Query = OptionQuery<T::Query>;
}

impl<T> DefaultQuery for Option<T>
where
    T: DefaultQuery,
{
    #[inline(always)]
    fn default_query() -> OptionQuery<T::Query> {
        OptionQuery(T::default_query())
    }
}

impl<T> AsQuery for OptionQuery<T>
where
    T: AsQuery,
{
    type Query = OptionQuery<T::Query>;
}

impl<T> IntoQuery for OptionQuery<T>
where
    T: IntoQuery,
{
    fn into_query(self) -> OptionQuery<T::Query> {
        OptionQuery(self.0.into_query())
    }
}

impl<T> DefaultQuery for OptionQuery<T>
where
    T: DefaultQuery,
{
    #[inline(always)]
    fn default_query() -> OptionQuery<T::Query> {
        OptionQuery(T::default_query())
    }
}

impl<T> QueryArg for OptionQuery<T>
where
    T: QueryArg,
{
    #[inline(always)]
    fn new() -> Self {
        OptionQuery(T::new())
    }
}

unsafe impl<T> Query for OptionQuery<T>
where
    T: Query,
{
    type Item<'a> = Option<T::Item<'a>>;
    type Fetch<'a> = Option<T::Fetch<'a>>;

    const MUTABLE: bool = T::MUTABLE;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        self.0.component_access(comp)
    }

    #[inline(always)]
    fn visit_archetype(&self, _: &Archetype) -> bool {
        true
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        if self.0.visit_archetype(archetype) {
            unsafe {
                self.0.access_archetype(archetype, f);
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
        if self.0.visit_archetype(archetype) && unsafe { self.0.visit_archetype_late(archetype) } {
            Some(unsafe { self.0.fetch(arch_idx, archetype, epoch) })
        } else {
            None
        }
    }

    fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<Option<T::Item<'a>>> {
        Some(self.0.reserved_entity_item(id, idx))
    }
}

unsafe impl<T> ImmutableQuery for OptionQuery<T> where T: ImmutableQuery {}
unsafe impl<T> SendQuery for OptionQuery<T> where T: SendQuery {}
