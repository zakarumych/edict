use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{split_idx, Archetype, Chunk},
    component::Component,
};

use super::{Fetch, ImmutableQuery, NonTrackingQuery, Query};

/// `Fetch` type for the `&T` query.
pub struct FetchRead<T> {
    pub(super) chunks: NonNull<Chunk>,
    pub(super) marker: PhantomData<fn() -> T>,
}

/// `Chunk` type for the `&T` query.
#[derive(Clone, Copy)]
pub struct ChunkRead<T> {
    ptr: NonNull<T>,
}

impl<'a, T> Fetch<'a> for FetchRead<T>
where
    T: Component,
{
    type Item = &'a T;
    type Chunk = ChunkRead<T>;

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> ChunkRead<T> {
        let chunk = &*self.chunks.as_ptr().add(idx);
        ChunkRead {
            ptr: chunk.ptr.cast(),
        }
    }

    #[inline]
    unsafe fn get_item(chunk: &ChunkRead<T>, idx: usize) -> &'a T {
        &*chunk.ptr.as_ptr().add(idx)
    }

    #[inline]
    unsafe fn get_one_item(&mut self, idx: u32) -> &'a T {
        let (chunk_idx, entity_idx) = split_idx(idx);
        let chunk = &mut *self.chunks.as_ptr().add(chunk_idx);
        &*chunk.ptr.cast::<T>().as_ptr().add(entity_idx)
    }
}

impl<T> Query for &T
where
    T: Component,
{
    type Fetch = FetchRead<T>;

    #[inline]
    fn mutates() -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _tracks: u64, _epoch: u64) -> Option<FetchRead<T>> {
        let idx = archetype.id_index(TypeId::of::<T>())?;
        let chunks = archetype.get_chunks(idx);

        Some(FetchRead {
            chunks: NonNull::from(&chunks[..]).cast(),
            marker: PhantomData,
        })
    }
}

unsafe impl<T> ImmutableQuery for &T where T: Component {}
unsafe impl<T> NonTrackingQuery for &T where T: Component {}
