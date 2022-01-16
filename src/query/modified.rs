use core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{split_idx, Archetype, Chunk, CHUNK_LEN_USIZE},
    component::Component,
};

use super::{
    alt::{Alt, RefMut},
    Fetch, ImmutableQuery, Query,
};

/// Query over modified component.
///
/// Should be used as either `Modified<&T>`, `Modified<&mut T>`
/// or `Modified<Alt<T>>`.
///
/// This is tracking query that requires providing subscriber's
/// `Tracks` to skip components that are not modified since the last time
/// that `Tracks` instance was used.
pub struct Modifed<T> {
    marker: PhantomData<fn() -> T>,
}

/// `Fetch` type for the `Modified<&T>` query.
pub struct ModifiedFetchRead<T> {
    tracks: u64,
    chunks: NonNull<Chunk>,
    marker: PhantomData<fn() -> T>,
}

/// `Chunk` type for the `Modified` query.
pub struct ModifiedChunk<T> {
    tracks: u64,
    ptr: NonNull<T>,
    versions: NonNull<[u64; CHUNK_LEN_USIZE]>,
}

impl<'a, T> Fetch<'a> for ModifiedFetchRead<T>
where
    T: 'a,
{
    type Item = &'a T;
    type Chunk = ModifiedChunk<T>;

    #[inline]
    unsafe fn skip_chunk(&self, idx: usize) -> bool {
        let chunk = &*self.chunks.as_ptr().add(idx);
        chunk.unmodified(self.tracks)
    }

    #[inline]
    unsafe fn skip_item(chunk: &ModifiedChunk<T>, idx: usize) -> bool {
        (*chunk.versions.as_ptr())[idx] <= chunk.tracks
    }

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> ModifiedChunk<T> {
        let chunk = &mut *self.chunks.as_ptr().add(idx);
        let ptr = chunk.ptr.cast();
        let versions = NonNull::from(&mut chunk.versions);

        ModifiedChunk {
            tracks: self.tracks,
            ptr,
            versions,
        }
    }

    #[inline]
    unsafe fn get_item(chunk: &ModifiedChunk<T>, idx: usize) -> &'a T {
        &*chunk.ptr.as_ptr().add(idx)
    }

    #[inline]
    unsafe fn get_one_item(&mut self, idx: u32) -> &'a T {
        let (chunk_idx, entity_idx) = split_idx(idx);
        let chunk = &mut *self.chunks.as_ptr().add(chunk_idx);
        &*chunk.ptr.cast::<T>().as_ptr().add(entity_idx)
    }
}

impl<T> Query for Modifed<&T>
where
    T: Component,
{
    type Fetch = ModifiedFetchRead<T>;

    #[inline]
    fn mutates() -> bool {
        false
    }

    #[inline]
    fn tracks() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(
        archetype: &Archetype,
        tracks: u64,
        _epoch: u64,
    ) -> Option<ModifiedFetchRead<T>> {
        let idx = archetype.id_index(TypeId::of::<T>())?;
        let chunks = archetype.get_chunks(idx);

        Some(ModifiedFetchRead {
            chunks: NonNull::from(&chunks[..]).cast(),
            tracks,
            marker: PhantomData,
        })
    }
}

unsafe impl<T> ImmutableQuery for Modifed<&T> where T: Component {}

/// `Fetch` type for the `Modified<&mut T>` query.
pub struct ModifiedFetchWrite<T> {
    tracks: u64,
    epoch: u64,
    chunks: NonNull<Chunk>,
    marker: PhantomData<fn() -> T>,
}

impl<'a, T> Fetch<'a> for ModifiedFetchWrite<T>
where
    T: 'a,
{
    type Item = &'a mut T;
    type Chunk = ModifiedChunk<T>;

    #[inline]
    unsafe fn skip_chunk(&self, idx: usize) -> bool {
        let chunk = &*self.chunks.as_ptr().add(idx);
        chunk.unmodified(self.tracks)
    }

    #[inline]
    unsafe fn skip_item(chunk: &ModifiedChunk<T>, idx: usize) -> bool {
        (*chunk.versions.as_ptr())[idx] <= chunk.tracks
    }

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> ModifiedChunk<T> {
        let chunk = &mut *self.chunks.as_ptr().add(idx);
        chunk.version = self.epoch;

        let ptr = chunk.ptr.cast();
        let versions = NonNull::from(&mut chunk.versions);

        ModifiedChunk {
            tracks: self.tracks,
            ptr,
            versions,
        }
    }

    #[inline]
    unsafe fn get_item(chunk: &ModifiedChunk<T>, idx: usize) -> &'a mut T {
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

impl<T> Query for Modifed<&mut T>
where
    T: Component,
{
    type Fetch = ModifiedFetchRead<T>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    fn tracks() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(
        archetype: &Archetype,
        tracks: u64,
        _epoch: u64,
    ) -> Option<ModifiedFetchRead<T>> {
        let idx = archetype.id_index(TypeId::of::<T>())?;
        let chunks = archetype.get_chunks_mut(idx);

        Some(ModifiedFetchRead {
            chunks: NonNull::from(&mut chunks[..]).cast(),
            tracks,
            marker: PhantomData,
        })
    }
}

pub struct ModifiedChunkAlt<'a, T> {
    tracks: u64,
    epoch: u64,
    ptr: NonNull<T>,
    versions: NonNull<[u64; CHUNK_LEN_USIZE]>,
    version: &'a Cell<u64>,
}

/// `Fetch` type for the `Modified<Alt<T>>` query.
pub struct ModifiedFetchAlt<T> {
    tracks: u64,
    epoch: u64,
    chunks: NonNull<Chunk>,
    marker: PhantomData<fn() -> T>,
}

impl<'a, T> Fetch<'a> for ModifiedFetchAlt<T>
where
    T: 'a,
{
    type Item = RefMut<'a, T>;
    type Chunk = ModifiedChunkAlt<'a, T>;

    #[inline]
    unsafe fn skip_chunk(&self, idx: usize) -> bool {
        let chunk = &*self.chunks.as_ptr().add(idx);
        chunk.unmodified(self.tracks)
    }

    #[inline]
    unsafe fn skip_item(chunk: &ModifiedChunkAlt<'a, T>, idx: usize) -> bool {
        (*chunk.versions.as_ptr())[idx] <= chunk.tracks
    }

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> ModifiedChunkAlt<'a, T> {
        let chunk = &mut *self.chunks.as_ptr().add(idx);

        let ptr = chunk.ptr.cast();
        let versions = NonNull::from(&mut chunk.versions);
        let version = Cell::from_mut(&mut chunk.version);

        ModifiedChunkAlt {
            epoch: self.epoch,
            tracks: self.tracks,
            ptr,
            versions,
            version,
        }
    }

    #[inline]
    unsafe fn get_item(chunk: &ModifiedChunkAlt<'a, T>, idx: usize) -> RefMut<'a, T> {
        RefMut {
            component: &mut *chunk.ptr.as_ptr().add(idx),
            entity_version: &mut (*chunk.versions.as_ptr())[idx],
            chunk_version: chunk.version,
            epoch: chunk.epoch,
        }
    }

    #[inline]
    unsafe fn get_one_item(&mut self, idx: u32) -> RefMut<'a, T> {
        let (chunk_idx, entity_idx) = split_idx(idx);
        let chunk = &mut *self.chunks.as_ptr().add(chunk_idx);

        RefMut {
            component: &mut *chunk.ptr.cast::<T>().as_ptr().add(entity_idx),
            entity_version: &mut chunk.versions[entity_idx],
            chunk_version: Cell::from_mut(&mut chunk.version),
            epoch: self.epoch,
        }
    }
}

impl<T> Query for Modifed<Alt<T>>
where
    T: Component,
{
    type Fetch = ModifiedFetchRead<T>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    fn tracks() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(
        archetype: &Archetype,
        tracks: u64,
        _epoch: u64,
    ) -> Option<ModifiedFetchRead<T>> {
        let idx = archetype.id_index(TypeId::of::<T>())?;
        let chunks = archetype.get_chunks_mut(idx);

        Some(ModifiedFetchRead {
            chunks: NonNull::from(&mut chunks[..]).cast(),
            tracks,
            marker: PhantomData,
        })
    }
}
