use crate::query::FetchCacheSlot;
use core::any::TypeId;

use crate::{
    archetype::Archetype, component::ComponentInfo, entity::EntityId, epoch::EpochId,
    query::DefaultQuery, system::QueryArg, world::World,
};

use super::{
    Access, AsQuery, BatchFetch, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias,
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
    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        if let Some(fetch) = self {
            unsafe { fetch.visit_chunk(chunk_idx) }
        } else {
            true
        }
    }

    /// Notifies this fetch that it visits a chunk.
    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        if let Some(fetch) = self {
            unsafe {
                fetch.touch_chunk(chunk_idx);
            }
        }
    }

    /// Checks if item with specified index must be visited or skipped.
    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        if let Some(fetch) = self {
            unsafe { fetch.visit_item(idx) }
        } else {
            true
        }
    }

    /// Returns fetched item at specified index.
    unsafe fn get_item(&mut self, idx: u32) -> Option<T::Item> {
        self.as_mut().map(|fetch| unsafe { fetch.get_item(idx) })
    }
}

unsafe impl<'a, T> BatchFetch<'a> for Option<T>
where
    T: BatchFetch<'a>,
{
    type Batch = Option<T::Batch>;

    /// Returns fetched item at specified index.
    unsafe fn get_batch(&mut self, start: u32, end: u32) -> Option<T::Batch> {
        self.as_mut()
            .map(|fetch| unsafe { fetch.get_batch(start, end) })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OptionQuery<T>(pub T);

impl<T> AsQuery for Option<T>
where
    T: AsQuery,
{
    type Query = OptionQuery<T::Query>;
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
    #[inline]
    fn default_query() -> Self {
        OptionQuery(T::default_query())
    }
}

impl<T> QueryArg for OptionQuery<T>
where
    T: QueryArg,
{
    #[inline]
    fn new() -> Self {
        OptionQuery(T::new())
    }

    #[inline]
    fn before(&mut self, world: &World) {
        self.0.before(world);
    }

    #[inline]
    fn after(&mut self, world: &World) {
        self.0.after(world);
    }
}

unsafe impl<T> Query for OptionQuery<T>
where
    T: Query,
{
    type Item<'a> = Option<T::Item<'a>>;
    type Fetch<'a> = Option<T::Fetch<'a>>;

    const MUTABLE: bool = T::MUTABLE;

    const SUPPORTS_FETCH_REUSE: bool = T::SUPPORTS_FETCH_REUSE;
    const FETCH_REUSE_MATCHES_STABLE: bool = T::FETCH_REUSE_MATCHES_STABLE;

    #[inline]
    unsafe fn reuse_fetch<'cached, 'a>(
        &self,
        cached: &mut Self::Fetch<'cached>,
        _arch_idx: u32,
        _archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Self::Fetch<'a> {
        if T::FETCH_REUSE_MATCHES_STABLE {
            return cached
                .as_mut()
                .map(|inner| unsafe { self.0.reuse_fetch(inner, _arch_idx, _archetype, epoch) });
        }
        if !self.0.visit_archetype(_archetype)
            || !unsafe { self.0.visit_archetype_late(_archetype) }
        {
            return None;
        }
        Some(if let Some(inner) = cached {
            unsafe { self.0.reuse_fetch(inner, _arch_idx, _archetype, epoch) }
        } else {
            unsafe { self.0.fetch(_arch_idx, _archetype, epoch) }
        })
    }
    const FETCH_CACHE_SLOTS: usize = T::FETCH_CACHE_SLOTS;

    #[inline]
    fn prepare_fetch_cache(archetype: &Archetype, slots: &mut [FetchCacheSlot]) {
        T::prepare_fetch_cache(archetype, slots);
    }

    #[inline]
    unsafe fn fetch_cached<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
        slots: &[FetchCacheSlot],
    ) -> Self::Fetch<'a> {
        unsafe {
            self.0
                .fetch_optional_cached(arch_idx, archetype, epoch, slots)
        }
    }

    #[inline]
    fn cache_visit_archetype(_archetype: &Archetype) -> Option<bool> {
        Some(true)
    }

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        self.0.component_access(comp)
    }

    #[inline]
    fn visit_archetype(&self, _: &Archetype) -> bool {
        true
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        unsafe {
            self.0.access_archetype(archetype, f);
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Option<T::Fetch<'a>> {
        unsafe { self.0.fetch_optional(arch_idx, archetype, epoch) }
    }

    fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<Option<T::Item<'a>>> {
        Some(self.0.reserved_entity_item(id, idx))
    }
}

unsafe impl<T> ImmutableQuery for OptionQuery<T> where T: ImmutableQuery {}
unsafe impl<T> SendQuery for OptionQuery<T> where T: SendQuery {}
