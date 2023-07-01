use core::any::TypeId;

use crate::{archetype::Archetype, epoch::EpochId};

use super::{Access, Fetch, ImmutablePhantomQuery, PhantomQuery};

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
            fetch.visit_chunk(chunk_idx)
        } else {
            true
        }
    }

    /// Notifies this fetch that it visits a chunk.
    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        if let Some(fetch) = self {
            fetch.touch_chunk(chunk_idx);
        }
    }

    /// Checks if item with specified index must be visited or skipped.
    #[inline]
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

unsafe impl<T> PhantomQuery for Option<T>
where
    T: PhantomQuery,
{
    type Item<'a> = Option<T::Item<'a>>;
    type Fetch<'a> = Option<T::Fetch<'a>>;

    const MUTABLE: bool = T::MUTABLE;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        T::access(ty)
    }

    #[inline]
    fn visit_archetype(_: &Archetype) -> bool {
        true
    }

    #[inline]
    unsafe fn access_archetype(archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        if T::visit_archetype(archetype) {
            T::access_archetype(archetype, f)
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Option<T::Fetch<'a>> {
        if !T::visit_archetype(archetype) {
            None
        } else {
            Some(T::fetch(arch_idx, archetype, epoch))
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for Option<T> where T: ImmutablePhantomQuery {}
