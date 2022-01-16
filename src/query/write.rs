use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{split_idx, Archetype, Chunk, CHUNK_LEN_USIZE},
    component::Component,
};

use super::{Fetch, NonTrackingQuery, Query};

/// `Fetch` type for the `&mut T` query.
pub struct FetchWrite<T> {
    pub(super) epoch: u64,
    pub(super) chunks: NonNull<Chunk>,
    pub(super) marker: PhantomData<fn() -> T>,
}

/// `Chunk` type for the `&mut T` query.
#[derive(Clone, Copy)]
pub struct ChunkWrite<T> {
    epoch: u64,
    ptr: NonNull<T>,
    versions: NonNull<[u64; CHUNK_LEN_USIZE]>,
}

impl<'a, T> Fetch<'a> for FetchWrite<T>
where
    T: Component,
{
    type Item = &'a mut T;
    type Chunk = ChunkWrite<T>;

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> ChunkWrite<T> {
        let chunk = &mut *self.chunks.as_ptr().add(idx);

        let ptr = chunk.ptr.cast();
        let versions = NonNull::from(&mut chunk.versions);
        chunk.version = self.epoch;

        ChunkWrite {
            epoch: self.epoch,
            ptr,
            versions,
        }
    }

    #[inline]
    unsafe fn get_item(chunk: &ChunkWrite<T>, idx: usize) -> &'a mut T {
        (*chunk.versions.as_ptr())[idx] = chunk.epoch;
        &mut *chunk.ptr.as_ptr().add(idx)
    }

    #[inline]
    unsafe fn get_one_item(&mut self, idx: u32) -> &'a mut T {
        let (chunk_idx, entity_idx) = split_idx(idx);

        let chunk = &mut *self.chunks.as_ptr().add(chunk_idx);
        chunk.version = self.epoch;
        chunk.versions[entity_idx] = self.epoch;
        &mut *chunk.ptr.cast::<T>().as_ptr().add(entity_idx)
    }
}

impl<T> Query for &mut T
where
    T: Component,
{
    type Fetch = FetchWrite<T>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    fn tracks() -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _tracks: u64, epoch: u64) -> Option<FetchWrite<T>> {
        let idx = archetype.id_index(TypeId::of::<T>())?;
        let chunks = archetype.get_chunks_mut(idx);

        Some(FetchWrite {
            epoch,
            chunks: NonNull::from(&mut chunks[..]).cast(),
            marker: PhantomData,
        })
    }
}

unsafe impl<T> NonTrackingQuery for &mut T where T: Component {}
