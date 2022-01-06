use core::{any::TypeId, iter::Enumerate, marker::PhantomData, slice};

use crate::{
    archetype::{split_idx, Archetype, EntityData, CHUNK_LEN_USIZE},
    entity::EntityId,
};

mod alt;
mod modified;
mod option;
mod read;
mod skip;
mod write;

pub use self::{alt::*, modified::*, option::*, read::*, skip::*, write::*};

pub enum Access {
    Shared(TypeId),
    Exclusive(TypeId),
}

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

pub trait Query {
    type Fetch: for<'a> Fetch<'a>;

    fn mutates() -> bool;

    #[inline]
    fn tracks() -> bool {
        false
    }

    unsafe fn fetch(archetype: &Archetype, tracks: u64, epoch: u64) -> Option<Self::Fetch>;
}

/// Query that is not mutates components.
///
/// # Safety
///
/// `Query::mutate` must return `false`.
/// `Query` must not borrow components mutably.
/// `Query` must not change entities versions.
pub unsafe trait ImmutableQuery {}

/// Query that is does not track component changes.
///
/// # Safety
///
/// `Query::tracks` must return `false`.
/// `Query` must not skip entities based on their versions.
pub unsafe trait NonTrackingQuery {}

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

pub struct QueryMut<'a, Q> {
    pub(crate) epoch: u64,
    pub(crate) archetypes: &'a [Archetype],
    pub(crate) query: PhantomData<Q>,
}

impl<'a, Q> QueryMut<'a, Q>
where
    Q: Query,
{
    pub fn iter(&mut self) -> QueryMutIter<'_, Q> {
        QueryMutIter {
            epoch: self.epoch,
            archetypes: self.archetypes.iter(),
            fetch: None,
            chunk: None,
            entities: [].iter().enumerate(),
        }
    }
}

impl<'a, Q> IntoIterator for QueryMut<'a, Q>
where
    Q: Query,
{
    type IntoIter = QueryMutIter<'a, Q>;
    type Item = (EntityId, QueryItem<'a, Q>);

    fn into_iter(self) -> QueryMutIter<'a, Q> {
        QueryMutIter {
            epoch: self.epoch,
            archetypes: self.archetypes.iter(),
            fetch: None,
            chunk: None,
            entities: [].iter().enumerate(),
        }
    }
}

pub struct QueryMutIter<'a, Q: Query> {
    epoch: u64,
    archetypes: slice::Iter<'a, Archetype>,

    fetch: Option<<Q as Query>::Fetch>,
    chunk: Option<<<Q as Query>::Fetch as Fetch<'a>>::Chunk>,
    entities: Enumerate<slice::Iter<'a, EntityData>>,
}

impl<'a, Q> Iterator for QueryMutIter<'a, Q>
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

pub struct QueryTrackedMut<'a, Q> {
    pub(crate) tracks: u64,
    pub(crate) epoch: u64,
    pub(crate) archetypes: &'a [Archetype],
    pub(crate) query: PhantomData<Q>,
}

impl<'a, Q> IntoIterator for QueryTrackedMut<'a, Q>
where
    Q: Query,
{
    type IntoIter = QueryTrackedMutIter<'a, Q>;
    type Item = (EntityId, QueryItem<'a, Q>);

    fn into_iter(self) -> QueryTrackedMutIter<'a, Q> {
        QueryTrackedMutIter {
            tracks: self.tracks,
            epoch: self.epoch,
            archetypes: self.archetypes.iter(),
            fetch: None,
            chunk: None,
            entities: [].iter().enumerate(),
        }
    }
}

pub struct QueryTrackedMutIter<'a, Q: Query> {
    tracks: u64,
    epoch: u64,
    archetypes: slice::Iter<'a, Archetype>,

    fetch: Option<<Q as Query>::Fetch>,
    chunk: Option<<<Q as Query>::Fetch as Fetch<'a>>::Chunk>,
    entities: Enumerate<slice::Iter<'a, EntityData>>,
}

impl<'a, Q> Iterator for QueryTrackedMutIter<'a, Q>
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
