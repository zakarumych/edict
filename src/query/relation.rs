use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    relation::{Origin, OriginComponent, Relation},
};

use super::{fetch::Fetch, phantom::PhantomQuery, Access, ImmutablePhantomQuery, Query};

/// Query to select entities with specified relation.
#[derive(Debug)]
pub struct Related<R> {
    marker: PhantomData<R>,
}

/// Fetch for the `Related<R>` query.
#[allow(missing_debug_implementations)]
pub struct RelatedFetchRead<R: Relation> {
    pub(super) ptr: NonNull<OriginComponent<R>>,
}

/// Iterator over relations of a given type on one entity.
#[allow(missing_debug_implementations)]
pub struct RelatedReadIter<'a, R> {
    iter: core::slice::Iter<'a, Origin<R>>,
}

impl<'a, R> Iterator for RelatedReadIter<'a, R> {
    type Item = (&'a R, EntityId);

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    #[inline]
    fn next(&mut self) -> Option<(&'a R, EntityId)> {
        let origin = self.iter.next()?;
        Some((&origin.relation, origin.target))
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<(&'a R, EntityId)> {
        let origin = self.iter.nth(n)?;
        Some((&origin.relation, origin.target))
    }

    #[inline]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.fold(init, |acc, origin| {
            f(acc, (&origin.relation, origin.target))
        })
    }
}

impl<'a, R> DoubleEndedIterator for RelatedReadIter<'a, R> {
    #[inline]
    fn next_back(&mut self) -> Option<(&'a R, EntityId)> {
        let origin = self.iter.next_back()?;
        Some((&origin.relation, origin.target))
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<(&'a R, EntityId)> {
        let origin = self.iter.nth_back(n)?;
        Some((&origin.relation, origin.target))
    }

    #[inline]
    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.rfold(init, |acc, origin| {
            f(acc, (&origin.relation, origin.target))
        })
    }
}

impl<'a, R> ExactSizeIterator for RelatedReadIter<'a, R> {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

unsafe impl<'a, R> Fetch<'a> for RelatedFetchRead<R>
where
    R: Relation,
{
    type Item = RelatedReadIter<'a, R>;

    #[inline]
    fn dangling() -> Self {
        RelatedFetchRead {
            ptr: NonNull::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn skip_item(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RelatedReadIter<'a, R> {
        let origin_component = &*self.ptr.as_ptr().add(idx);

        RelatedReadIter {
            iter: origin_component.origins().iter(),
        }
    }
}

unsafe impl<R> PhantomQuery for Related<&R>
where
    R: Relation,
{
    type Fetch = RelatedFetchRead<R>;

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn conflicts<Q>(query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(
            query.access(TypeId::of::<OriginComponent<R>>()),
            Some(Access::Write)
        )
    }

    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _epoch: u64) -> RelatedFetchRead<R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<OriginComponent<R>>());

        RelatedFetchRead {
            ptr: data.ptr.cast(),
        }
    }
}

unsafe impl<R> ImmutablePhantomQuery for Related<&R> where R: Relation {}

/// Returns relation reading query not bound to target entity.
/// Yields all relations of a given type on entities.
///
/// To get relation with specific entity use [`related_to`].
pub fn related<'a, R>() -> PhantomData<Related<&'a R>>
where
    R: Relation,
{
    PhantomData
}
