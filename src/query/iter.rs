use core::{ops::Range, ptr::NonNull, slice};

use crate::{
    archetype::{chunk_idx, first_of_chunk, Archetype, CHUNK_LEN_USIZE},
    entity::EntityId,
};

use super::{fetch::Fetch, Query, QueryFetch, QueryItem};

/// Iterator over entities with a query `Q`.
/// Yields `EntityId` and query items for every matching entity.
#[allow(missing_debug_implementations)]
pub struct QueryIter<'a, Q: Query> {
    query: Q,
    epoch: u64,
    archetypes: slice::Iter<'a, Archetype>,
    fetch: <Q as QueryFetch<'a>>::Fetch,
    entities: NonNull<EntityId>,
    indices: Range<usize>,
    visit_chunk: bool,
}

impl<'a, Q> QueryIter<'a, Q>
where
    Q: Query,
{
    pub(crate) fn new(query: Q, epoch: u64, archetypes: &'a [Archetype]) -> Self {
        QueryIter {
            query,
            epoch,
            archetypes: archetypes.iter(),
            fetch: <Q as QueryFetch<'a>>::Fetch::dangling(),
            entities: NonNull::dangling(),
            indices: 0..0,
            visit_chunk: false,
        }
    }
}

impl<'a, Q> Iterator for QueryIter<'a, Q>
where
    Q: Query,
{
    type Item = (EntityId, QueryItem<'a, Q>);

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let upper = self
            .archetypes
            .clone()
            .fold(self.indices.len(), |acc, archetype| {
                if self.query.skip_archetype_unconditionally(archetype) {
                    return acc;
                }
                acc + archetype.len()
            });

        (0, Some(upper))
    }

    #[inline]
    fn next(&mut self) -> Option<(EntityId, QueryItem<'a, Q>)> {
        loop {
            match self.indices.next() {
                None => {
                    // move to the next archetype.
                    loop {
                        let archetype = self.archetypes.next()?;

                        if archetype.is_empty() {
                            continue;
                        }

                        if self.query.skip_archetype_unconditionally(archetype) {
                            continue;
                        }

                        self.fetch = unsafe { self.query.fetch(archetype, self.epoch) };
                        self.entities = NonNull::from(archetype.entities()).cast();
                        self.indices = 0..archetype.len();
                        break;
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
                        let entity = unsafe { *self.entities.as_ptr().add(idx) };

                        return Some((entity, item));
                    }
                }
            }
        }
    }

    fn fold<B, Fun>(mut self, init: B, mut f: Fun) -> B
    where
        Self: Sized,
        Fun: FnMut(B, (EntityId, QueryItem<'a, Q>)) -> B,
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
                let entity = unsafe { *self.entities.as_ptr().add(idx as usize) };

                acc = f(acc, (entity, item));
            }
        }

        for archetype in self.archetypes {
            if archetype.is_empty() {
                continue;
            }
            if self.query.skip_archetype_unconditionally(archetype) {
                continue;
            }
            let mut fetch = unsafe { self.query.fetch(archetype, self.epoch) };

            let entities = archetype.entities().as_ptr();
            let mut indices = 0..archetype.len();

            while let Some(idx) = indices.next() {
                if let Some(chunk_idx) = first_of_chunk(idx) {
                    if unsafe { fetch.skip_chunk(chunk_idx) } {
                        self.indices.nth(CHUNK_LEN_USIZE - 1);
                        continue;
                    }
                    self.visit_chunk = true;
                }

                if !unsafe { fetch.skip_item(idx) } {
                    if self.visit_chunk {
                        unsafe { fetch.visit_chunk(chunk_idx(idx)) }
                        self.visit_chunk = false;
                    }
                    let item = unsafe { fetch.get_item(idx) };
                    let entity = unsafe { *entities.add(idx) };

                    acc = f(acc, (entity, item));
                }
            }
        }
        acc
    }
}
