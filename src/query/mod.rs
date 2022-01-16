pub use self::{
    alt::{Alt, ChunkAlt, FetchAlt},
    modified::{
        Modifed, ModifiedChunk, ModifiedChunkAlt, ModifiedFetchAlt, ModifiedFetchRead,
        ModifiedFetchWrite,
    },
    read::FetchRead,
    write::{ChunkWrite, FetchWrite},
};

use core::{iter::Enumerate, slice};

use crate::{
    archetype::{split_idx, Archetype, CHUNK_LEN_USIZE},
    entity::{EntityId, WeakEntity},
};

mod alt;
mod modified;
mod option;
mod read;
mod skip;
mod write;

pub use self::{alt::*, modified::*, option::*, read::*, skip::*, write::*};

/// Trait implemented for `Query::Fetch` associated types.
pub trait Fetch<'a> {
    type Chunk;
    type Item;

    #[inline]
    unsafe fn skip_chunk(&self, _idx: usize) -> bool {
        false
    }

    unsafe fn get_chunk(&mut self, idx: usize) -> Self::Chunk;

    #[inline]
    unsafe fn skip_item(_chunk: &Self::Chunk, _idx: usize) -> bool {
        false
    }

    unsafe fn get_item(chunk: &Self::Chunk, idx: usize) -> Self::Item;

    unsafe fn get_one_item(&mut self, idx: u32) -> Self::Item;
}

/// Trait for types that can query sets of components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// sets of references to the components and optionally `WeakEntity` to address same components later.
pub trait Query {
    type Fetch: for<'a> Fetch<'a>;

    fn mutates() -> bool;

    #[inline]
    fn tracks() -> bool {
        false
    }

    unsafe fn fetch(archetype: &Archetype, tracks: u64, epoch: u64) -> Option<Self::Fetch>;
}

/// Query that does not mutate any components.
///
/// # Safety
///
/// `Query::mutate` must return `false`.
/// `Query` must not borrow components mutably.
/// `Query` must not change entities versions.
pub unsafe trait ImmutableQuery {}

/// Query that does not track component changes.
///
/// # Safety
///
/// `Query::tracks` must return `false`.
/// `Query` must not skip entities based on their versions.
pub unsafe trait NonTrackingQuery {}

/// Type alias for items returned by query type.
pub type QueryItem<'a, Q> = <<Q as Query>::Fetch as Fetch<'a>>::Item;

macro_rules! for_tuple {
    () => {
        for_tuple!(for A B C D E F G H);
    };

    (for) => {
        for_tuple!(impl);
    };

    (for $head:ident $($tail:ident)*) => {
        for_tuple!(for $($tail)*);
        for_tuple!(impl $head $($tail)*);
    };

    (impl) => {
        impl Fetch<'_> for () {
            type Item = ();
            type Chunk = ();

            #[inline]
            unsafe fn get_chunk(&mut self, _: usize) {}

            #[inline]
            unsafe fn get_item(_: &Self::Chunk, _: usize) {}

            #[inline]
            unsafe fn get_one_item(&mut self, _: u32) {}
        }

        impl Query for () {
            type Fetch = ();

            #[inline]
            fn mutates() -> bool {
                false
            }

            #[inline]
            fn tracks() -> bool {
                false
            }

            #[inline]
            unsafe fn fetch(_: & Archetype, _: u64, _: u64) -> Option<()> {
                Some(())
            }
        }

        unsafe impl ImmutableQuery for () {}
        unsafe impl NonTrackingQuery for () {}
    };

    (impl $($a:ident)+) => {
        impl<'a $(, $a)+> Fetch<'a> for ($($a,)+)
        where $($a: Fetch<'a>,)+
        {
            type Chunk = ($($a::Chunk,)+);
            type Item = ($($a::Item,)+);

            #[inline]
            unsafe fn get_chunk(&mut self, idx: usize) -> ($($a::Chunk,)+) {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                ($( $a::get_chunk($a, idx), )+)
            }

            #[inline]
            unsafe fn get_item(chunk: &($($a::Chunk,)+), idx: usize) -> ($($a::Item,)+) {
                #[allow(non_snake_case)]
                let ($($a,)+) = chunk;
                ($( $a::get_item($a, idx), )+)
            }

            #[inline]
            unsafe fn get_one_item(&mut self, idx: u32) -> ($($a::Item,)+) {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                ($( $a::get_one_item($a, idx), )+)
            }
        }

        impl<$($a),+> Query for ($($a,)+) where $($a: Query,)+ {
            type Fetch = ($($a::Fetch,)+);

            #[inline]
            fn mutates() -> bool {
                false $( || $a::mutates()) +
            }

            #[inline]
            fn tracks() -> bool {
                false $( || $a::tracks()) +
            }

            #[inline]
            unsafe fn fetch(archetype: & Archetype, track: u64, epoch: u64) -> Option<($($a::Fetch,)+)> {
                Some(($( $a::fetch(archetype, track, epoch)?, )+))
            }
        }

        unsafe impl<$($a),+> ImmutableQuery for ($($a,)+) where $($a: ImmutableQuery,)+ {}
        unsafe impl<$($a),+> NonTrackingQuery for ($($a,)+) where $($a: NonTrackingQuery,)+ {}
    };
}

for_tuple!();

/// Iterator over entities with a query `Q`.
/// Yields `WeakEntity` and query items for every matching entity.
///
/// Supports only `NonTrackingQuery`.
pub struct QueryIter<'a, Q: Query> {
    epoch: u64,
    archetypes: slice::Iter<'a, Archetype>,

    fetch: Option<<Q as Query>::Fetch>,
    chunk: Option<<<Q as Query>::Fetch as Fetch<'a>>::Chunk>,
    entities: Enumerate<slice::Iter<'a, WeakEntity>>,
}

impl<'a, Q> QueryIter<'a, Q>
where
    Q: Query,
{
    pub(crate) fn new(epoch: u64, archetypes: &'a [Archetype]) -> Self {
        QueryIter {
            epoch,
            archetypes: archetypes.iter(),
            fetch: None,
            chunk: None,
            entities: [].iter().enumerate(),
        }
    }
}

impl<'a, Q> Iterator for QueryIter<'a, Q>
where
    Q: Query,
{
    type Item = (EntityId, QueryItem<'a, Q>);

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.entities.len()))
    }

    fn next(&mut self) -> Option<(EntityId, QueryItem<'a, Q>)> {
        loop {
            match self.entities.next() {
                None => {
                    // move to the next archetype.
                    loop {
                        let archetype = self.archetypes.next()?;
                        self.fetch = unsafe { Q::fetch(archetype, 0, self.epoch) };

                        if self.fetch.is_some() {
                            self.entities = archetype.get_entities().iter().enumerate();
                            break;
                        }
                    }
                }
                Some((idx, entity)) if idx <= u32::MAX as usize => {
                    let (chunk_idx, entity_idx) = split_idx(idx as u32);

                    if entity_idx == 0 {
                        let fetch = self.fetch.as_mut().unwrap();

                        if unsafe { fetch.skip_chunk(chunk_idx) } {
                            continue;
                        }

                        self.chunk = Some(unsafe { fetch.get_chunk(chunk_idx) });
                    }

                    let chunk = self.chunk.as_mut().unwrap();

                    if unsafe { Q::Fetch::skip_item(chunk, entity_idx) } {
                        continue;
                    }

                    let item = unsafe { Q::Fetch::get_item(chunk, entity_idx) };

                    return Some((EntityId { id: entity.id }, item));
                }
                Some((_, _)) => {
                    panic!("Entity index is too large");
                }
            }
        }
    }
}

/// Iterator over entities with a query `Q`.
/// Yields `WeakEntity` and query items for every matching entity.
///
/// Does not require `Q` to implement `NonTrackingQuery`.
pub struct QueryTrackedIter<'a, Q: Query> {
    tracks: u64,
    epoch: u64,
    archetypes: slice::Iter<'a, Archetype>,

    fetch: Option<<Q as Query>::Fetch>,
    chunk: Option<<<Q as Query>::Fetch as Fetch<'a>>::Chunk>,
    entities: Enumerate<slice::Iter<'a, WeakEntity>>,
}

impl<'a, Q> QueryTrackedIter<'a, Q>
where
    Q: Query,
{
    pub(crate) fn new(tracks: u64, epoch: u64, archetypes: &'a [Archetype]) -> Self {
        QueryTrackedIter {
            tracks,
            epoch,
            archetypes: archetypes.iter(),
            fetch: None,
            chunk: None,
            entities: [].iter().enumerate(),
        }
    }
}

impl<'a, Q> Iterator for QueryTrackedIter<'a, Q>
where
    Q: Query,
{
    type Item = (EntityId, QueryItem<'a, Q>);

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.entities.len()))
    }

    fn next(&mut self) -> Option<(EntityId, QueryItem<'a, Q>)> {
        loop {
            match self.entities.next() {
                None => {
                    // move to the next archetype.
                    loop {
                        let archetype = self.archetypes.next()?;
                        self.fetch = unsafe { Q::fetch(archetype, self.tracks, self.epoch) };

                        if self.fetch.is_some() {
                            self.entities = archetype.get_entities().iter().enumerate();
                            break;
                        }
                    }
                }
                Some((idx, entity)) if idx <= u32::MAX as usize => {
                    let (chunk_idx, entity_idx) = split_idx(idx as u32);
                    if entity_idx == 0 {
                        let fetch = self.fetch.as_mut().unwrap();

                        if unsafe { fetch.skip_chunk(chunk_idx) } {
                            self.entities.nth(CHUNK_LEN_USIZE - 1);
                            continue;
                        }

                        self.chunk = Some(unsafe { fetch.get_chunk(chunk_idx) });
                    }

                    let chunk = self.chunk.as_mut().unwrap();

                    if unsafe { Q::Fetch::skip_item(chunk, entity_idx) } {
                        continue;
                    }

                    let item = unsafe { Q::Fetch::get_item(chunk, entity_idx) };

                    return Some((EntityId { id: entity.id }, item));
                }
                Some((_, _)) => {
                    panic!("Entity index is too large");
                }
            }
        }
    }
}
