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
        self.borrow
            .acquire(&self.query, &self.filter, self.archetypes);

        let epoch = self.epochs.next_if(Q::MUTABLE || F::MUTABLE);
        ViewIter::new(
            self.query,
            self.filter,
            epoch,
            self.archetypes,
            StaticallyBorrowed, // Borrowed in `ViewState` for the duration of returned lifetime.
        )
    }
}

impl<'a, Q, F, B> IntoIterator for ViewValue<'a, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    type Item = QueryItem<'a, Q>;

    type IntoIter = ViewIter<'a, Q, F, B>;

    #[inline(always)]
    fn into_iter(self) -> ViewIter<'a, Q, F, B> {
        self.borrow
            .acquire(&self.query, &self.filter, self.archetypes);

        let epoch = self.epochs.next_if(Q::MUTABLE || F::MUTABLE);
        ViewIter::new(self.query, self.filter, epoch, self.archetypes, self.borrow)
    }
}

/// Iterator over entities with a query `Q`.
/// Yields query items for every matching entity.
pub struct ViewIter<'a, Q: Query, F: Query, B = StaticallyBorrowed> {
    query: Q,
    query_fetch: Q::Fetch<'a>,
    filter: F,
    filter_fetch: F::Fetch<'a>,
    epoch: EpochId,
    archetypes_iter: core::iter::Enumerate<core::slice::Iter<'a, Archetype>>,
    indices: Range<u32>,
    touch_chunk: bool,
    #[allow(dead_code)]
    borrow: B,
}

impl<'a, Q, F, B> ViewIter<'a, Q, F, B>
where
    Q: Query,
    F: Query,
{
    pub(crate) fn new(
        query: Q,
        filter: F,
        epoch: EpochId,
        archetypes: &'a [Archetype],
        borrow: B,
    ) -> Self {
        ViewIter {
            query,
            query_fetch: Fetch::dangling(),
            filter,
            filter_fetch: Fetch::dangling(),
            epoch,
            archetypes_iter: archetypes.iter().enumerate(),
            indices: 0..0,
            touch_chunk: false,
            borrow,
        }
    }
}

impl<'a, Q, F, B> Iterator for ViewIter<'a, Q, F, B>
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
