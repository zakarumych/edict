use core::{cell::Cell, ptr::NonNull};

use crate::{
    archetype::{split_idx, Archetype},
    component::Component,
};

use super::{
    alt::{Alt, ChunkAlt, FetchAlt, RefMut},
    read::FetchRead,
    write::FetchWrite,
    ChunkWrite, Fetch, ImmutableQuery, NonTrackingQuery, Query,
};

impl<'a, T> Fetch<'a> for Option<FetchRead<T>>
where
    T: Component,
{
    type Item = Option<&'a T>;
    type Chunk = Option<NonNull<T>>;

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> Option<NonNull<T>> {
        Some(self.as_mut()?.get_chunk(idx))
    }

    #[inline]
    unsafe fn get_item(chunk: &Option<NonNull<T>>, idx: usize) -> Option<&'a T> {
        Some(FetchRead::get_item(chunk.as_ref()?, idx))
    }

    #[inline]
    unsafe fn get_one_item(&mut self, idx: u32) -> Option<&'a T> {
        let (chunk_idx, entity_idx) = split_idx(idx);
        let chunk = &mut *self.as_mut()?.chunks.as_ptr().add(chunk_idx);
        Some(&*chunk.ptr.cast::<T>().as_ptr().add(entity_idx))
    }
}

impl<T> Query for Option<&T>
where
    T: Component,
{
    type Fetch = Option<FetchRead<T>>;

    #[inline]
    fn mutates() -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(
        archetype: &Archetype,
        tracks: u64,
        epoch: u64,
    ) -> Option<Option<FetchRead<T>>> {
        Some(<&T as Query>::fetch(archetype, tracks, epoch))
    }
}

unsafe impl<T> ImmutableQuery for Option<&T> where T: Component {}
unsafe impl<T> NonTrackingQuery for Option<&T> where T: Component {}

impl<'a, T> Fetch<'a> for Option<FetchWrite<T>>
where
    T: Component,
{
    type Item = Option<&'a mut T>;
    type Chunk = Option<ChunkWrite<T>>;

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> Option<ChunkWrite<T>> {
        Some(self.as_mut()?.get_chunk(idx))
    }

    #[inline]
    unsafe fn get_item(chunk: &Option<ChunkWrite<T>>, idx: usize) -> Option<&'a mut T> {
        Some(FetchWrite::get_item(chunk.as_ref()?, idx))
    }

    #[inline]
    unsafe fn get_one_item(&mut self, idx: u32) -> Option<&'a mut T> {
        Some(FetchWrite::get_one_item(self.as_mut()?, idx))
    }
}

impl<T> Query for Option<&mut T>
where
    T: Component,
{
    type Fetch = Option<FetchWrite<T>>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(
        archetype: &Archetype,
        track: u64,
        epoch: u64,
    ) -> Option<Option<FetchWrite<T>>> {
        Some(<&mut T as Query>::fetch(archetype, track, epoch))
    }
}

unsafe impl<T> NonTrackingQuery for Option<&mut T> where T: Component {}

impl<'a, T> Fetch<'a> for Option<FetchAlt<T>>
where
    T: Component,
{
    type Item = Option<RefMut<'a, T>>;
    type Chunk = Option<ChunkAlt<'a, T>>;

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> Option<ChunkAlt<'a, T>> {
        Some(self.as_mut()?.get_chunk(idx))
    }

    #[inline]
    unsafe fn get_item(chunk: &Option<ChunkAlt<'a, T>>, idx: usize) -> Option<RefMut<'a, T>> {
        Some(FetchAlt::get_item(chunk.as_ref()?, idx))
    }

    #[inline]
    unsafe fn get_one_item(&mut self, idx: u32) -> Option<RefMut<'a, T>> {
        let (chunk_idx, entity_idx) = split_idx(idx);
        let fetch = self.as_mut()?;
        let chunk = &mut *fetch.chunks.as_ptr().add(chunk_idx);

        Some(RefMut {
            component: &mut *chunk.ptr.cast::<T>().as_ptr().add(entity_idx),
            entity_version: &mut chunk.versions[entity_idx],
            chunk_version: Cell::from_mut(&mut chunk.version),
            epoch: fetch.epoch,
        })
    }
}

impl<T> Query for Option<Alt<T>>
where
    T: Component,
{
    type Fetch = Option<FetchAlt<T>>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, track: u64, epoch: u64) -> Option<Option<FetchAlt<T>>> {
        Some(<Alt<T> as Query>::fetch(archetype, track, epoch))
    }
}

unsafe impl<T> NonTrackingQuery for Option<Alt<T>> where T: Component {}
