//! Queries and iterators.
//!
//! To efficiently iterate over entities with specific set of components,
//! or only over thoses where specific component is modified, or missing,
//! [`Query`] is the solution.
//!
//! [`Query`] trait has a lot of implementations and is composable using tuples.

pub use self::{
    alt::{Alt, FetchAlt},
    modified::{Modifed, ModifiedFetchAlt, ModifiedFetchRead, ModifiedFetchWrite},
    read::FetchRead,
    write::FetchWrite,
};

use core::{
    ops::Range,
    ptr::{self},
    slice,
};

use crate::{
    archetype::{chunk_idx, first_of_chunk, Archetype, CHUNK_LEN_USIZE},
    entity::WeakEntity,
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
    /// Item type this fetch type yields.
    type Item;

    /// Returns `Fetch` value that must not be used.
    fn dangling() -> Self;

    /// Checks if chunk with specified index must be skipped.
    #[inline]
    unsafe fn skip_chunk(&self, chunk_idx: usize) -> bool {
        drop(chunk_idx);
        false
    }

    /// Checks if item with specified index must be skipped.
    #[inline]
    unsafe fn skip_item(&self, idx: usize) -> bool {
        drop(idx);
        false
    }

    /// Notifies this fetch that it visits a chunk.
    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        drop(chunk_idx);
    }

    /// Returns fetched item at specifeid index.
    unsafe fn get_item(&mut self, idx: usize) -> Self::Item;
}

/// Trait for types that can query sets of components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// sets of references to the components and optionally `WeakEntity` to address same components later.
pub trait Query {
    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch: for<'a> Fetch<'a>;

    /// Checks if this query type mutates any of the components.
    /// Queries that returns [`false`] must never attempt to modify a component.
    /// [`ImmutableQuery`] must statically return [`false`]
    /// and never attempt to modify a component.
    #[inline]
    fn mutates() -> bool {
        false
    }

    /// Checks if this query tracks changes of any of the components.
    #[inline]
    fn tracks() -> bool {
        false
    }

    /// Fetches data from one archetype.
    /// Returns [`None`] is archetype does not match query requirements.
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

            #[inline]
            fn dangling() {}

            #[inline]
            unsafe fn get_item(&mut self, _idx: usize) {}
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
            type Item = ($($a::Item,)+);

            #[inline]
            fn dangling() -> Self {
                ($($a::dangling(),)+)
            }

            #[inline]
            unsafe fn get_item(&mut self, idx: usize) -> ($($a::Item,)+) {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                ($( $a.get_item(idx), )+)
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
#[allow(missing_debug_implementations)]
pub struct QueryIter<'a, Q: Query> {
    epoch: u64,
    archetypes: slice::Iter<'a, Archetype>,

    fetch: <Q as Query>::Fetch,
    entities: *const WeakEntity,
    indices: Range<usize>,
}

impl<'a, Q> QueryIter<'a, Q>
where
    Q: Query,
{
    pub(crate) fn new(epoch: u64, archetypes: &'a [Archetype]) -> Self {
        QueryIter {
            epoch,
            archetypes: archetypes.iter(),
            fetch: Q::Fetch::dangling(),
            entities: ptr::null(),
            indices: 0..0,
        }
    }
}

impl<'a, Q> Iterator for QueryIter<'a, Q>
where
    Q: Query,
{
    type Item = (WeakEntity, QueryItem<'a, Q>);

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.indices.len() as usize, None)
    }

    #[inline]
    fn next(&mut self) -> Option<(WeakEntity, QueryItem<'a, Q>)> {
        loop {
            match self.indices.next() {
                None => {
                    // move to the next archetype.
                    loop {
                        let archetype = self.archetypes.next()?;
                        if let Some(fetch) = unsafe { Q::fetch(archetype, 0, self.epoch) } {
                            self.fetch = fetch;
                            self.entities = archetype.entities().as_ptr();
                            self.indices = 0..archetype.len();
                            break;
                        }
                    }
                }
                Some(idx) => {
                    if let Some(chunk_idx) = first_of_chunk(idx) {
                        unsafe { self.fetch.visit_chunk(chunk_idx) }
                    }

                    debug_assert!(!unsafe { self.fetch.skip_item(idx) });

                    let item = unsafe { self.fetch.get_item(idx) };
                    let entity = unsafe { *self.entities.add(idx) };

                    return Some((entity, item));
                }
            }
        }
    }

    fn fold<B, F>(mut self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, (WeakEntity, QueryItem<'a, Q>)) -> B,
    {
        let mut acc = init;
        for idx in self.indices {
            if let Some(chunk_idx) = first_of_chunk(idx) {
                unsafe { self.fetch.visit_chunk(chunk_idx) }
            }
            debug_assert!(!unsafe { self.fetch.skip_item(idx) });

            let item = unsafe { self.fetch.get_item(idx) };
            let entity = unsafe { *self.entities.add(idx as usize) };

            acc = f(acc, (entity, item));
        }

        for archetype in self.archetypes {
            if let Some(mut fetch) = unsafe { Q::fetch(archetype, 0, self.epoch) } {
                let entities = archetype.entities().as_ptr();

                for idx in 0..archetype.len() {
                    if let Some(chunk_idx) = first_of_chunk(idx) {
                        unsafe { self.fetch.visit_chunk(chunk_idx) }
                    }
                    debug_assert!(!unsafe { fetch.skip_item(idx) });

                    let item = unsafe { fetch.get_item(idx) };
                    let entity = unsafe { *entities.add(idx) };

                    acc = f(acc, (entity, item));
                }
            }
        }
        acc
    }
}

/// Iterator over entities with a query `Q`.
/// Yields `WeakEntity` and query items for every matching entity.
///
/// Does not require `Q` to implement `NonTrackingQuery`.
#[allow(missing_debug_implementations)]
pub struct QueryTrackedIter<'a, Q: Query> {
    tracks: u64,
    epoch: u64,
    archetypes: slice::Iter<'a, Archetype>,

    fetch: <Q as Query>::Fetch,
    entities: *const WeakEntity,
    indices: Range<usize>,
    visit_chunk: bool,
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
            fetch: Q::Fetch::dangling(),
            entities: ptr::null(),
            indices: 0..0,
            visit_chunk: false,
        }
    }
}

impl<'a, Q> Iterator for QueryTrackedIter<'a, Q>
where
    Q: Query,
{
    type Item = (WeakEntity, QueryItem<'a, Q>);

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }

    #[inline]
    fn next(&mut self) -> Option<(WeakEntity, QueryItem<'a, Q>)> {
        loop {
            match self.indices.next() {
                None => {
                    // move to the next archetype.
                    loop {
                        let archetype = self.archetypes.next()?;
                        if let Some(fetch) = unsafe { Q::fetch(archetype, self.tracks, self.epoch) }
                        {
                            self.fetch = fetch;
                            self.entities = archetype.entities().as_ptr();
                            self.indices = 0..archetype.len();
                            break;
                        }
                    }
                }
                Some(idx) => {
                    if let Some(chunk_idx) = first_of_chunk(idx) {
                        if unsafe { self.fetch.skip_chunk(chunk_idx) } {
                            self.indices.nth(CHUNK_LEN_USIZE - 1);
                            continue;
                        }
                        self.visit_chunk = true;
                    }

                    if !unsafe { self.fetch.skip_item(idx) } {
                        if self.visit_chunk {
                            unsafe { self.fetch.visit_chunk(chunk_idx(idx)) }
                            self.visit_chunk = false;
                        }

                        let item = unsafe { self.fetch.get_item(idx) };
                        let entity = unsafe { *self.entities.add(idx) };

                        return Some((entity, item));
                    }
                }
            }
        }
    }

    fn fold<B, F>(mut self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, (WeakEntity, QueryItem<'a, Q>)) -> B,
    {
        let mut acc = init;
        while let Some(idx) = self.indices.next() {
            if let Some(chunk_idx) = first_of_chunk(idx) {
                if unsafe { self.fetch.skip_chunk(chunk_idx) } {
                    self.indices.nth(CHUNK_LEN_USIZE - 1);
                    continue;
                }
                self.visit_chunk = true;
            }

            if !unsafe { self.fetch.skip_item(idx) } {
                if self.visit_chunk {
                    unsafe { self.fetch.visit_chunk(chunk_idx(idx)) }
                    self.visit_chunk = false;
                }
                let item = unsafe { self.fetch.get_item(idx) };
                let entity = unsafe { *self.entities.add(idx as usize) };

                acc = f(acc, (entity, item));
            }
        }

        for archetype in self.archetypes {
            if let Some(mut fetch) = unsafe { Q::fetch(archetype, 0, self.epoch) } {
                let entities = archetype.entities().as_ptr();
                let mut indices = 0..archetype.len();

                while let Some(idx) = indices.next() {
                    if let Some(chunk_idx) = first_of_chunk(idx) {
                        if unsafe { self.fetch.skip_chunk(chunk_idx) } {
                            self.indices.nth(CHUNK_LEN_USIZE - 1);
                            continue;
                        }
                        self.visit_chunk = true;
                    }

                    if !unsafe { fetch.skip_item(idx) } {
                        if self.visit_chunk {
                            unsafe { self.fetch.visit_chunk(chunk_idx(idx)) }
                            self.visit_chunk = false;
                        }
                        let item = unsafe { fetch.get_item(idx) };
                        let entity = unsafe { *entities.add(idx) };

                        acc = f(acc, (entity, item));
                    }
                }
            }
        }
        acc
    }
}
