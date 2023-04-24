use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, PhantomQuery},
    relation::{Origin, OriginComponent, Relation},
};

phantom_newtype! {
    /// Query for origins of relation.
    ///
    /// Yields iterator of pairs - relation instance and target.
    pub struct Relates<R>
}

impl<R> Relates<&R>
where
    R: Relation + Sync,
{
    /// Creates a new [`Relates`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

impl<R> Relates<&mut R>
where
    R: Relation + Send,
{
    /// Creates a new [`Relates`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

/// Iterator over relations of a given type on one entity.
#[derive(Clone)]
pub struct RelatesReadIter<'a, R> {
    iter: core::slice::Iter<'a, Origin<R>>,
}

impl<'a, R> Iterator for RelatesReadIter<'a, R> {
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

impl<'a, R> DoubleEndedIterator for RelatesReadIter<'a, R> {
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

impl<'a, R> ExactSizeIterator for RelatesReadIter<'a, R> {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Fetch for the [`Relates<&R>`] query.
pub struct FetchRelatesRead<'a, R: Relation> {
    ptr: NonNull<OriginComponent<R>>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesRead<'a, R>
where
    R: Relation + Sync,
{
    type Item = RelatesReadIter<'a, R>;

    #[inline]
    fn dangling() -> Self {
        FetchRelatesRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RelatesReadIter<'a, R> {
        let origin_component = &*self.ptr.as_ptr().add(idx);

        RelatesReadIter {
            iter: origin_component.origins().iter(),
        }
    }
}

unsafe impl<R> PhantomQuery for Relates<&R>
where
    R: Relation + Sync,
{
    type Item<'a> = RelatesReadIter<'a, R>;
    type Fetch<'a> = FetchRelatesRead<'a, R>;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<OriginComponent<R>>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: EpochId) -> FetchRelatesRead<'a, R> {
        let component = archetype
            .component(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();

        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = component.data();

        FetchRelatesRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutablePhantomQuery for Relates<&R> where R: Relation + Sync {}

/// Iterator over relations of a given type on one entity.
pub struct RelatesWriteIter<'a, R> {
    iter: core::slice::IterMut<'a, Origin<R>>,
}

impl<'a, R> Iterator for RelatesWriteIter<'a, R> {
    type Item = (&'a mut R, EntityId);

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    #[inline]
    fn next(&mut self) -> Option<(&'a mut R, EntityId)> {
        let origin = self.iter.next()?;
        Some((&mut origin.relation, origin.target))
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<(&'a mut R, EntityId)> {
        let origin = self.iter.nth(n)?;
        Some((&mut origin.relation, origin.target))
    }

    #[inline]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.fold(init, |acc, origin| {
            f(acc, (&mut origin.relation, origin.target))
        })
    }
}

impl<'a, R> DoubleEndedIterator for RelatesWriteIter<'a, R> {
    #[inline]
    fn next_back(&mut self) -> Option<(&'a mut R, EntityId)> {
        let origin = self.iter.next_back()?;
        Some((&mut origin.relation, origin.target))
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<(&'a mut R, EntityId)> {
        let origin = self.iter.nth_back(n)?;
        Some((&mut origin.relation, origin.target))
    }

    #[inline]
    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.rfold(init, |acc, origin| {
            f(acc, (&mut origin.relation, origin.target))
        })
    }
}

impl<'a, R> ExactSizeIterator for RelatesWriteIter<'a, R> {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Fetch for the [`Relates<&mut R>`] query.
pub struct FetchRelatesWrite<'a, R: Relation> {
    epoch: EpochId,
    ptr: NonNull<OriginComponent<R>>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesWrite<'a, R>
where
    R: Relation + Send,
{
    type Item = RelatesWriteIter<'a, R>;

    #[inline]
    fn dangling() -> Self {
        FetchRelatesWrite {
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: usize) {
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_epoch.bump(self.epoch);
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RelatesWriteIter<'a, R> {
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx);
        entity_epoch.bump(self.epoch);

        let origin_component = &mut *self.ptr.as_ptr().add(idx);

        RelatesWriteIter {
            iter: origin_component.origins_mut().iter_mut(),
        }
    }
}

unsafe impl<R> PhantomQuery for Relates<&mut R>
where
    R: Relation + Send,
{
    type Item<'a> = RelatesWriteIter<'a, R>;
    type Fetch<'a> = FetchRelatesWrite<'a, R>;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<OriginComponent<R>>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: EpochId) -> FetchRelatesWrite<'a, R> {
        let component = archetype
            .component(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = component.data_mut();
        data.epoch.bump(epoch);

        FetchRelatesWrite {
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            marker: PhantomData,
        }
    }
}
