use core::{
    any::TypeId,
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    archetype::{split_idx, Archetype, Chunk, CHUNK_LEN_USIZE},
    component::Component,
};

use super::{Fetch, NonTrackingQuery, Query};

/// Query type that is an alternative to `&mut T`.
/// Yields mutable reference wrapper that bumps component version on dereference.
/// In contrast with `&mut T` that bumps component version on yield, but works faster.
/// Use this query if redundant version bumps would cause heavy calculations.
///
/// `Alt` is `NonTrackingQuery` as it does not depend on current versions
/// of the components.
pub struct Alt<T>(PhantomData<T>);

/// Item type that `Alt` yields.
/// Wraps `&mut T` and implements `DerefMut` to `T`.
/// Bumps component version on dereference.
pub struct RefMut<'a, T: ?Sized> {
    pub(super) component: &'a mut T,
    pub(super) entity_version: &'a mut u64,
    pub(super) chunk_version: &'a Cell<u64>,
    pub(super) epoch: u64,
}

impl<T> Deref for RefMut<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &*self.component
    }
}

impl<T> DerefMut for RefMut<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        *self.entity_version = self.epoch;
        self.chunk_version.set(self.epoch);
        self.component
    }
}

/// `Fetch` type for the `Alt` query.
pub struct FetchAlt<T> {
    pub(super) epoch: u64,
    pub(super) chunks: NonNull<Chunk>,
    pub(super) marker: PhantomData<fn() -> T>,
}

/// `Chunk` type for the `Alt` query.
#[derive(Clone, Copy)]
pub struct ChunkAlt<'a, T> {
    epoch: u64,
    ptr: NonNull<T>,
    versions: NonNull<[u64; CHUNK_LEN_USIZE]>,
    version: &'a Cell<u64>,
}

impl<'a, T> Fetch<'a> for FetchAlt<T>
where
    T: Component,
{
    type Item = RefMut<'a, T>;
    type Chunk = ChunkAlt<'a, T>;

    #[inline]
    unsafe fn get_chunk(&mut self, idx: usize) -> ChunkAlt<'a, T> {
        let chunk = &mut *self.chunks.as_ptr().add(idx);
        let ptr = chunk.ptr.cast();
        let versions = NonNull::from(&mut chunk.versions);
        let version = Cell::from_mut(&mut chunk.version);

        ChunkAlt {
            epoch: self.epoch,
            ptr,
            versions,
            version,
        }
    }

    #[inline]
    unsafe fn get_item(chunk: &ChunkAlt<'a, T>, idx: usize) -> RefMut<'a, T> {
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

impl<T> Query for Alt<T>
where
    T: Component,
{
    type Fetch = FetchAlt<T>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _tracks: u64, epoch: u64) -> Option<FetchAlt<T>> {
        let idx = archetype.id_index(TypeId::of::<T>())?;
        let chunks = archetype.get_chunks_mut(idx);

        Some(FetchAlt {
            epoch,
            chunks: NonNull::from(&mut chunks[..]).cast(),
            marker: PhantomData,
        })
    }
}

unsafe impl<T> NonTrackingQuery for Alt<T> {}
