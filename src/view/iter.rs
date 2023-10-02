use core::ops::Range;

use crate::{
    archetype::{chunk_idx, first_of_chunk, Archetype, CHUNK_LEN},
    epoch::EpochId,
    query::{Fetch, Query, QueryItem},
};

use super::{BorrowState, StaticallyBorrowed, ViewValue};

impl<'a, Q, F, B> ViewValue<'a, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    /// Returns an iterator over entities with a query `Q` and filter `F`.
    #[inline(always)]
    pub fn iter(&self) -> ViewIter<'_, Q, F> {
        let epoch = self.epochs.next_if(Q::MUTABLE || F::MUTABLE);

        self.acquire_borrow();

        // Safety: we just acquired the borrow. Releasing requires a mutable reference to self.
        // This ensures that it can only happen after the iterator is dropped.
        unsafe { ViewIter::new(epoch, self.query, self.filter, self.archetypes) }
    }
}

impl<'a, Q, F> IntoIterator for ViewValue<'a, Q, F, StaticallyBorrowed>
where
    Q: Query,
    F: Query,
{
    type Item = QueryItem<'a, Q>;
    type IntoIter = ViewIter<'a, Q, F>;

    #[inline(always)]
    fn into_iter(self) -> ViewIter<'a, Q, F> {
        let epoch = self.epochs.next_if(Q::MUTABLE || F::MUTABLE);

        // Safety: data is statically borrowed for 'a.
        unsafe { ViewIter::new(epoch, self.query, self.filter, self.archetypes) }
    }
}

/// Iterator over entities with a query `Q`.
/// Yields query items for every matching entity.
pub struct ViewIter<'a, Q: Query, F: Query> {
    query: Q,
    filter: F,
    query_fetch: Q::Fetch<'a>,
    filter_fetch: F::Fetch<'a>,
    epoch: EpochId,
    archetypes_iter: core::iter::Enumerate<core::slice::Iter<'a, Archetype>>,
    indices: Range<u32>,
    touch_chunk: bool,
}

impl<'a, Q, F> ViewIter<'a, Q, F>
where
    Q: Query,
    F: Query,
{
    /// Creates a new iterator over entities with a query `Q` and filter `F`.
    ///
    /// # Safety
    ///
    /// The caller must ensure that data for the query and filter is borrowed for the duration of the iterator,
    /// i.e. lifetime 'a.
    unsafe fn new(epoch: EpochId, query: Q, filter: F, archetypes: &'a [Archetype]) -> Self {
        ViewIter {
            query,
            filter,
            query_fetch: Fetch::dangling(),
            filter_fetch: Fetch::dangling(),
            epoch,
            archetypes_iter: archetypes.iter().enumerate(),
            indices: 0..0,
            touch_chunk: false,
        }
    }
}

impl<'a, Q, F> Iterator for ViewIter<'a, Q, F>
where
    Q: Query,
    F: Query,
{
    type Item = QueryItem<'a, Q>;

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let upper = self
            .archetypes_iter
            .clone()
            .fold(self.indices.len(), |acc, (_, archetype)| {
                if !self.filter.visit_archetype(archetype) {
                    return acc;
                }
                if !self.query.visit_archetype(archetype) {
                    return acc;
                }
                acc + archetype.len()
            });

        (0, Some(upper))
    }

    #[inline(always)]
    fn next(&mut self) -> Option<QueryItem<'a, Q>> {
        loop {
            match self.indices.next() {
                None => {
                    // move to the next archetype.
                    loop {
                        let (arch_idx, archetype) = self.archetypes_iter.next()?;

                        if archetype.is_empty() {
                            continue;
                        }

                        if !self.filter.visit_archetype(archetype) {
                            continue;
                        }

                        if !self.query.visit_archetype(archetype) {
                            continue;
                        }

                        self.filter_fetch =
                            unsafe { self.filter.fetch(arch_idx as u32, archetype, self.epoch) };
                        self.query_fetch =
                            unsafe { self.query.fetch(arch_idx as u32, archetype, self.epoch) };
                        self.indices = 0..archetype.len() as u32;
                        break;
                    }
                }
                Some(entity_idx) => {
                    if let Some(chunk_idx) = first_of_chunk(entity_idx) {
                        if !unsafe { self.filter_fetch.visit_chunk(chunk_idx) } {
                            self.indices.nth(CHUNK_LEN as usize - 1);
                            continue;
                        }
                        if !unsafe { self.query_fetch.visit_chunk(chunk_idx) } {
                            self.indices.nth(CHUNK_LEN as usize - 1);
                            continue;
                        }
                        self.touch_chunk = true;
                    }

                    if !unsafe { self.filter_fetch.visit_item(entity_idx) } {
                        continue;
                    }

                    if !unsafe { self.query_fetch.visit_item(entity_idx) } {
                        continue;
                    }

                    if self.touch_chunk {
                        unsafe { self.filter_fetch.touch_chunk(chunk_idx(entity_idx)) }
                        unsafe { self.query_fetch.touch_chunk(chunk_idx(entity_idx)) }
                        self.touch_chunk = false;
                    }

                    let item = unsafe { self.query_fetch.get_item(entity_idx) };

                    return Some(item);
                }
            }
        }
    }

    fn fold<I, Fun>(mut self, init: I, mut f: Fun) -> I
    where
        Self: Sized,
        Fun: FnMut(I, QueryItem<'a, Q>) -> I,
    {
        let mut acc = init;
        while let Some(entity_idx) = self.indices.next() {
            if let Some(chunk_idx) = first_of_chunk(entity_idx) {
                if !unsafe { self.filter_fetch.visit_chunk(chunk_idx) } {
                    self.indices.nth(CHUNK_LEN as usize - 1);
                    continue;
                }
                if !unsafe { self.query_fetch.visit_chunk(chunk_idx) } {
                    self.indices.nth(CHUNK_LEN as usize - 1);
                    continue;
                }
                self.touch_chunk = true;
            }

            if !unsafe { self.filter_fetch.visit_item(entity_idx) } {
                continue;
            }
            if !unsafe { self.query_fetch.visit_item(entity_idx) } {
                continue;
            }

            if self.touch_chunk {
                unsafe { self.filter_fetch.touch_chunk(chunk_idx(entity_idx)) }
                unsafe { self.query_fetch.touch_chunk(chunk_idx(entity_idx)) }
                self.touch_chunk = false;
            }
            let item = unsafe { self.query_fetch.get_item(entity_idx) };

            acc = f(acc, item);
        }

        for (arch_idx, archetype) in self.archetypes_iter.by_ref() {
            if archetype.is_empty() {
                continue;
            }
            if !self.filter.visit_archetype(archetype) {
                continue;
            }
            if !self.query.visit_archetype(archetype) {
                continue;
            }
            let mut filter_fetch =
                unsafe { self.filter.fetch(arch_idx as u32, archetype, self.epoch) };
            let mut query_fetch =
                unsafe { self.query.fetch(arch_idx as u32, archetype, self.epoch) };

            let mut indices = 0..archetype.len() as u32;

            while let Some(entity_idx) = indices.next() {
                if let Some(chunk_idx) = first_of_chunk(entity_idx) {
                    if !unsafe { query_fetch.visit_chunk(chunk_idx) } {
                        self.indices.nth(CHUNK_LEN as usize - 1);
                        continue;
                    }
                    if !unsafe { filter_fetch.visit_chunk(chunk_idx) } {
                        self.indices.nth(CHUNK_LEN as usize - 1);
                        continue;
                    }
                    self.touch_chunk = true;
                }

                if !unsafe { filter_fetch.visit_item(entity_idx) } {
                    continue;
                }
                if !unsafe { query_fetch.visit_item(entity_idx) } {
                    continue;
                }

                if self.touch_chunk {
                    unsafe { filter_fetch.touch_chunk(chunk_idx(entity_idx)) }
                    unsafe { query_fetch.touch_chunk(chunk_idx(entity_idx)) }
                    self.touch_chunk = false;
                }
                let item = unsafe { query_fetch.get_item(entity_idx) };

                acc = f(acc, item);
            }
        }
        acc
    }
}
