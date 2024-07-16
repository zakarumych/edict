use crate::entity::{EntityBound, EntityId};

/// Iterator over relations of a given type on one entity.
#[derive(Clone)]
pub struct RelationIter<'a, R> {
    iter: core::slice::Iter<'a, (EntityId, R)>,
}

impl<'a, R> RelationIter<'a, R> {
    /// Creates a new iterator over relations of a given type on one entity.
    #[inline(always)]
    pub fn new(relations: &'a [(EntityId, R)]) -> Self {
        RelationIter {
            iter: relations.iter(),
        }
    }
}

impl<'a, R> Iterator for RelationIter<'a, R> {
    type Item = EntityBound<'a>;

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    #[inline(always)]
    fn next(&mut self) -> Option<EntityBound<'a>> {
        let origin = self.iter.next()?;
        Some(EntityBound::new(origin.0))
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<EntityBound<'a>> {
        let origin = self.iter.nth(n)?;
        Some(EntityBound::new(origin.0))
    }

    #[inline(always)]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter
            .fold(init, |acc, origin| f(acc, EntityBound::new(origin.0)))
    }
}

impl<'a, R> DoubleEndedIterator for RelationIter<'a, R> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<EntityBound<'a>> {
        let origin = self.iter.next_back()?;
        Some(EntityBound::new(origin.0))
    }

    #[inline(always)]
    fn nth_back(&mut self, n: usize) -> Option<EntityBound<'a>> {
        let origin = self.iter.nth_back(n)?;
        Some(EntityBound::new(origin.0))
    }

    #[inline(always)]
    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter
            .rfold(init, |acc, origin| f(acc, EntityBound::new(origin.0)))
    }
}

impl<'a, R> ExactSizeIterator for RelationIter<'a, R> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Iterator over relations of a given type on one entity.
#[derive(Clone)]
pub struct RelationReadIter<'a, R> {
    iter: core::slice::Iter<'a, (EntityId, R)>,
}

impl<'a, R> RelationReadIter<'a, R> {
    /// Creates a new iterator over relations of a given type on one entity.
    #[inline(always)]
    pub fn new(relations: &'a [(EntityId, R)]) -> Self {
        RelationReadIter {
            iter: relations.iter(),
        }
    }
}

impl<'a, R> Iterator for RelationReadIter<'a, R> {
    type Item = (&'a R, EntityBound<'a>);

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    #[inline(always)]
    fn next(&mut self) -> Option<(&'a R, EntityBound<'a>)> {
        let origin = self.iter.next()?;
        Some((&origin.1, EntityBound::new(origin.0)))
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<(&'a R, EntityBound<'a>)> {
        let origin = self.iter.nth(n)?;
        Some((&origin.1, EntityBound::new(origin.0)))
    }

    #[inline(always)]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.fold(init, |acc, origin| {
            f(acc, (&origin.1, EntityBound::new(origin.0)))
        })
    }
}

impl<'a, R> DoubleEndedIterator for RelationReadIter<'a, R> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<(&'a R, EntityBound<'a>)> {
        let origin = self.iter.next_back()?;
        Some((&origin.1, EntityBound::new(origin.0)))
    }

    #[inline(always)]
    fn nth_back(&mut self, n: usize) -> Option<(&'a R, EntityBound<'a>)> {
        let origin = self.iter.nth_back(n)?;
        Some((&origin.1, EntityBound::new(origin.0)))
    }

    #[inline(always)]
    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.rfold(init, |acc, origin| {
            f(acc, (&origin.1, EntityBound::new(origin.0)))
        })
    }
}

impl<'a, R> ExactSizeIterator for RelationReadIter<'a, R> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Iterator over relations of a given type on one entity.
pub struct RelationWriteIter<'a, R> {
    iter: core::slice::IterMut<'a, (EntityId, R)>,
}

impl<'a, R> RelationWriteIter<'a, R> {
    /// Creates a new iterator over relations of a given type on one entity.
    #[inline(always)]
    pub fn new(relations: &'a mut [(EntityId, R)]) -> Self {
        RelationWriteIter {
            iter: relations.iter_mut(),
        }
    }
}

impl<'a, R> Iterator for RelationWriteIter<'a, R> {
    type Item = (&'a mut R, EntityBound<'a>);

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    #[inline(always)]
    fn next(&mut self) -> Option<(&'a mut R, EntityBound<'a>)> {
        let origin = self.iter.next()?;
        Some((&mut origin.1, EntityBound::new(origin.0)))
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<(&'a mut R, EntityBound<'a>)> {
        let origin = self.iter.nth(n)?;
        Some((&mut origin.1, EntityBound::new(origin.0)))
    }

    #[inline(always)]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.fold(init, |acc, origin| {
            f(acc, (&mut origin.1, EntityBound::new(origin.0)))
        })
    }
}

impl<'a, R> DoubleEndedIterator for RelationWriteIter<'a, R> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<(&'a mut R, EntityBound<'a>)> {
        let origin = self.iter.next_back()?;
        Some((&mut origin.1, EntityBound::new(origin.0)))
    }

    #[inline(always)]
    fn nth_back(&mut self, n: usize) -> Option<(&'a mut R, EntityBound<'a>)> {
        let origin = self.iter.nth_back(n)?;
        Some((&mut origin.1, EntityBound::new(origin.0)))
    }

    #[inline(always)]
    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.rfold(init, |acc, origin| {
            f(acc, (&mut origin.1, EntityBound::new(origin.0)))
        })
    }
}

impl<'a, R> ExactSizeIterator for RelationWriteIter<'a, R> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.iter.len()
    }
}
