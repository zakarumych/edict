use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::{EntityBound, EntityId},
    epoch::EpochId,
    query::{DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, Read, Write, WriteAlias},
    relation::{OriginComponent, Relation, RelationTarget},
    Access,
};

marker_type! {
    /// Query for origins of relation.
    ///
    /// Yields iterator of pairs - relation instance and target.
    pub struct Relates<R>;
}

/// Iterator over relations of a given type on one entity.
#[derive(Clone)]
pub struct RelatesReadIter<'a, R> {
    iter: core::slice::Iter<'a, RelationTarget<R>>,
}

impl<'a, R> Iterator for RelatesReadIter<'a, R> {
    type Item = (&'a R, EntityBound<'a>);

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    #[inline(always)]
    fn next(&mut self) -> Option<(&'a R, EntityBound<'a>)> {
        let origin = self.iter.next()?;
        Some((&origin.relation, EntityBound::new(origin.target)))
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<(&'a R, EntityBound<'a>)> {
        let origin = self.iter.nth(n)?;
        Some((&origin.relation, EntityBound::new(origin.target)))
    }

    #[inline(always)]
    fn fold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.fold(init, |acc, origin| {
            f(acc, (&origin.relation, EntityBound::new(origin.target)))
        })
    }
}

impl<'a, R> DoubleEndedIterator for RelatesReadIter<'a, R> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<(&'a R, EntityBound<'a>)> {
        let origin = self.iter.next_back()?;
        Some((&origin.relation, EntityBound::new(origin.target)))
    }

    #[inline(always)]
    fn nth_back(&mut self, n: usize) -> Option<(&'a R, EntityBound<'a>)> {
        let origin = self.iter.nth_back(n)?;
        Some((&origin.relation, EntityBound::new(origin.target)))
    }

    #[inline(always)]
    fn rfold<B, F>(self, init: B, mut f: F) -> B
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        self.iter.rfold(init, |acc, origin| {
            f(acc, (&origin.relation, EntityBound::new(origin.target)))
        })
    }
}

impl<'a, R> ExactSizeIterator for RelatesReadIter<'a, R> {
    #[inline(always)]
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

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatesRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> RelatesReadIter<'a, R> {
        let origin_component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };

        RelatesReadIter {
            iter: origin_component.relations().iter(),
        }
    }
}

impl<R> IntoQuery for Relates<&R>
where
    R: Relation + Sync,
{
    type Query = Relates<Read<R>>;

    #[inline(always)]
    fn into_query(self) -> Relates<Read<R>> {
        Relates
    }
}

impl<R> DefaultQuery for Relates<&R>
where
    R: Relation + Sync,
{
    #[inline(always)]
    fn default_query() -> Relates<Read<R>> {
        Relates
    }
}

impl<R> IntoQuery for Relates<Read<R>>
where
    R: Relation + Sync,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self::Query {
        self
    }
}

impl<R> DefaultQuery for Relates<Read<R>>
where
    R: Relation + Sync,
{
    #[inline(always)]
    fn default_query() -> Self {
        Relates
    }
}

unsafe impl<R> Query for Relates<Read<R>>
where
    R: Relation + Sync,
{
    type Item<'a> = RelatesReadIter<'a, R>;
    type Fetch<'a> = FetchRelatesRead<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_type_access(&self, ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(Access::read_type::<OriginComponent<R>>(ty))
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<OriginComponent<R>>(), Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelatesRead<'a, R> {
        let component = unsafe {
            archetype
                .component(TypeId::of::<OriginComponent<R>>())
                .unwrap_unchecked()
        };

        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = unsafe { component.data() };

        FetchRelatesRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for Relates<Read<R>> where R: Relation + Sync {}

/// Iterator over relations of a given type on one entity.
pub struct RelatesWriteIter<'a, R> {
    iter: core::slice::IterMut<'a, RelationTarget<R>>,
}

impl<'a, R> Iterator for RelatesWriteIter<'a, R> {
    type Item = (&'a mut R, EntityId);

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    #[inline(always)]
    fn next(&mut self) -> Option<(&'a mut R, EntityId)> {
        let origin = self.iter.next()?;
        Some((&mut origin.relation, origin.target))
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<(&'a mut R, EntityId)> {
        let origin = self.iter.nth(n)?;
        Some((&mut origin.relation, origin.target))
    }

    #[inline(always)]
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
    #[inline(always)]
    fn next_back(&mut self) -> Option<(&'a mut R, EntityId)> {
        let origin = self.iter.next_back()?;
        Some((&mut origin.relation, origin.target))
    }

    #[inline(always)]
    fn nth_back(&mut self, n: usize) -> Option<(&'a mut R, EntityId)> {
        let origin = self.iter.nth_back(n)?;
        Some((&mut origin.relation, origin.target))
    }

    #[inline(always)]
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
    #[inline(always)]
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

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatesWrite {
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_epoch.bump(self.epoch);
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> RelatesWriteIter<'a, R> {
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
        entity_epoch.bump(self.epoch);

        let origin_component = unsafe { &mut *self.ptr.as_ptr().add(idx as usize) };

        RelatesWriteIter {
            iter: origin_component.relations_mut().iter_mut(),
        }
    }
}

impl<R> IntoQuery for Relates<&mut R>
where
    R: Relation + Send,
{
    type Query = Relates<Write<R>>;

    #[inline(always)]
    fn into_query(self) -> Relates<Write<R>> {
        Relates
    }
}

impl<R> DefaultQuery for Relates<&mut R>
where
    R: Relation + Send,
{
    #[inline(always)]
    fn default_query() -> Relates<Write<R>> {
        Relates
    }
}

impl<R> IntoQuery for Relates<Write<R>>
where
    R: Relation + Send,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for Relates<Write<R>>
where
    R: Relation + Send,
{
    #[inline(always)]
    fn default_query() -> Self {
        Relates
    }
}

unsafe impl<R> Query for Relates<Write<R>>
where
    R: Relation + Send,
{
    type Item<'a> = RelatesWriteIter<'a, R>;
    type Fetch<'a> = FetchRelatesWrite<'a, R>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn component_type_access(&self, ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(Access::write_type::<OriginComponent<R>>(ty))
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<OriginComponent<R>>(), Access::Write)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchRelatesWrite<'a, R> {
        let component = unsafe {
            archetype
                .component(TypeId::of::<OriginComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = unsafe { component.data_mut() };
        data.epoch.bump(epoch);

        FetchRelatesWrite {
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
            chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
            marker: PhantomData,
        }
    }
}
