use core::{ops::Range, slice};

use crate::{
    archetype::{chunk_idx, first_of_chunk, Archetype, CHUNK_LEN_USIZE},
    epoch::EpochId,
};

use super::{fetch::Fetch, Query, QueryItem};

/// Iterator over entities with a query `Q`.
/// Yields query items for every matching entity.
pub struct QueryIter<'a, Q: Query> {
    query: Q,
    epoch: EpochId,
    archetypes_iter: slice::Iter<'a, Archetype>,
    fetch: Q::Fetch<'a>,
    indices: Range<usize>,
    visit_chunk: bool,
}

impl<'a, Q> QueryIter<'a, Q>
where
    Q: Query,
{
    pub(crate) fn new(query: Q, epoch: EpochId, archetypes: &'a [Archetype]) -> Self {
        QueryIter {
            query,
            epoch,
            archetypes_iter: archetypes.iter(),
            fetch: <Q::Fetch<'a>>::dangling(),
            indices: 0..0,
            visit_chunk: false,
        }
    }
}

impl<'a, Q> Iterator for QueryIter<'a, Q>
where
    Q: Query,
{
    type Item = QueryItem<'a, Q>;

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let upper = self
            .archetypes_iter
            .clone()
            .fold(self.indices.len(), |acc, archetype| {
                if !self.query.visit_archetype(archetype) {
                    return acc;
                }
                acc + archetype.len()
            });

        (0, Some(upper))
    }

    #[inline]
    fn next(&mut self) -> Option<QueryItem<'a, Q>> {
        loop {
            match self.indices.next() {
                None => {
                    // move to the next archetype.
                    loop {
                        let archetype = self.archetypes_iter.next()?;

                        if archetype.is_empty() {
                            continue;
                        }

                        if !self.query.visit_archetype(archetype) {
                            continue;
                        }

                        self.fetch = unsafe { self.query.fetch(archetype, self.epoch) };
                        self.indices = 0..archetype.len();
                        break;
                    }
                }
                Some(idx) => {
                    if let Some(chunk_idx) = first_of_chunk(idx) {
                        if !unsafe { self.fetch.visit_chunk(chunk_idx) } {
                            self.indices.nth(CHUNK_LEN_USIZE - 1);
                            continue;
                        }
                        self.visit_chunk = true;
                    }

                    if unsafe { self.fetch.visit_item(idx) } {
                        if self.visit_chunk {
                            unsafe { self.fetch.touch_chunk(chunk_idx(idx)) }
                            self.visit_chunk = false;
                        }

                        let item = unsafe { self.fetch.get_item(idx) };

                        return Some(item);
                    }
                }
            }
        }
    }

    fn fold<B, Fun>(mut self, init: B, mut f: Fun) -> B
    where
        Self: Sized,
        Fun: FnMut(B, QueryItem<'a, Q>) -> B,
    {
        let mut acc = init;
        while let Some(idx) = self.indices.next() {
            if let Some(chunk_idx) = first_of_chunk(idx) {
                if !unsafe { self.fetch.visit_chunk(chunk_idx) } {
                    self.indices.nth(CHUNK_LEN_USIZE - 1);
                    continue;
                }
                self.visit_chunk = true;
            }

            if unsafe { self.fetch.visit_item(idx) } {
                if self.visit_chunk {
                    unsafe { self.fetch.touch_chunk(chunk_idx(idx)) }
                    self.visit_chunk = false;
                }
                let item = unsafe { self.fetch.get_item(idx) };

                acc = f(acc, item);
            }
        }

        for archetype in self.archetypes_iter.by_ref() {
            if archetype.is_empty() {
                continue;
            }
            if !self.query.visit_archetype(archetype) {
                continue;
            }
            let mut fetch = unsafe { self.query.fetch(archetype, self.epoch) };

            let mut indices = 0..archetype.len();

            while let Some(idx) = indices.next() {
                if let Some(chunk_idx) = first_of_chunk(idx) {
                    if !unsafe { fetch.visit_chunk(chunk_idx) } {
                        self.indices.nth(CHUNK_LEN_USIZE - 1);
                        continue;
                    }
                    self.visit_chunk = true;
                }

                if unsafe { fetch.visit_item(idx) } {
                    if self.visit_chunk {
                        unsafe { fetch.touch_chunk(chunk_idx(idx)) }
                        self.visit_chunk = false;
                    }
                    let item = unsafe { fetch.get_item(idx) };

                    acc = f(acc, item);
                }
            }
        }
        acc
    }
}
