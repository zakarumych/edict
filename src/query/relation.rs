use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    relation::{Origin, OriginComponent, Relation},
};

use super::{
    fetch::Fetch, phantom::PhantomQuery, Access, ImmutablePhantomQuery, ImmutableQuery, Query,
};

phantom_newtype! {
    /// Query to select entities with specified relation.
    pub struct QueryRelation<R>
}

/// Iterator over relations of a given type on one entity.
#[allow(missing_debug_implementations)]
pub struct RelationReadIter<'a, R> {
    iter: core::slice::Iter<'a, Origin<R>>,
}

impl<'a, R> Iterator for RelationReadIter<'a, R> {
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

impl<'a, R> DoubleEndedIterator for RelationReadIter<'a, R> {
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

impl<'a, R> ExactSizeIterator for RelationReadIter<'a, R> {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Fetch for the `Related<R>` query.
#[allow(missing_debug_implementations)]
pub struct FetchRelationRead<R: Relation> {
    ptr: NonNull<OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelationRead<R>
where
    R: Relation,
{
    type Item = RelationReadIter<'a, R>;

    #[inline]
    fn dangling() -> Self {
        FetchRelationRead {
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
    unsafe fn get_item(&mut self, idx: usize) -> RelationReadIter<'a, R> {
        let origin_component = &*self.ptr.as_ptr().add(idx);

        RelationReadIter {
            iter: origin_component.origins().iter(),
        }
    }
}

unsafe impl<R> PhantomQuery for QueryRelation<&R>
where
    R: Relation,
{
    type Fetch = FetchRelationRead<R>;

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
    unsafe fn fetch(archetype: &Archetype, _epoch: u64) -> FetchRelationRead<R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<OriginComponent<R>>());

        FetchRelationRead {
            ptr: data.ptr.cast(),
        }
    }
}

unsafe impl<R> ImmutablePhantomQuery for QueryRelation<&R> where R: Relation {}

/// Returns relation reading query not bound to any target entity.
/// Yields all relations of a given type on entities.
///
/// To get relation with specific entity use [`read_relation_to`].
pub fn read_relation<'a, R>() -> PhantomData<QueryRelation<&'a R>>
where
    R: Relation,
{
    PhantomData
}

/// Iterator over relations of a given type on one entity.
#[allow(missing_debug_implementations)]
pub struct RelationWriteIter<'a, R> {
    iter: core::slice::IterMut<'a, Origin<R>>,
}

impl<'a, R> Iterator for RelationWriteIter<'a, R> {
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

impl<'a, R> DoubleEndedIterator for RelationWriteIter<'a, R> {
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

impl<'a, R> ExactSizeIterator for RelationWriteIter<'a, R> {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Fetch for the `Related<R>` query.
#[allow(missing_debug_implementations)]
pub struct FetchRelationWrite<R: Relation> {
    epoch: u64,
    ptr: NonNull<OriginComponent<R>>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelationWrite<R>
where
    R: Relation,
{
    type Item = RelationWriteIter<'a, R>;

    #[inline]
    fn dangling() -> Self {
        FetchRelationWrite {
            epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
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
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx);

        debug_assert!(*chunk_version < self.epoch);
        *chunk_version = self.epoch;
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RelationWriteIter<'a, R> {
        let entity_version = &mut *self.entity_versions.as_ptr().add(idx);

        debug_assert!(*entity_version < self.epoch);
        *entity_version = self.epoch;

        let origin_component = &mut *self.ptr.as_ptr().add(idx);

        RelationWriteIter {
            iter: origin_component.origins_mut().iter_mut(),
        }
    }
}

unsafe impl<R> PhantomQuery for QueryRelation<&mut R>
where
    R: Relation,
{
    type Fetch = FetchRelationWrite<R>;

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
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
            Some(Access::Read | Access::Write)
        )
    }

    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, epoch: u64) -> FetchRelationWrite<R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<OriginComponent<R>>());

        debug_assert!(*data.version.get() < epoch);
        *data.version.get() = epoch;

        FetchRelationWrite {
            epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions,
        }
    }
}

/// Returns relation writing query not bound to any target entity.
/// Yields all relations of a given type on entities.
///
/// To get relation with specific entity use [`write_relation_to`].
pub fn write_relation<'a, R>() -> PhantomData<QueryRelation<&'a mut R>>
where
    R: Relation,
{
    PhantomData
}

/// Returns relation reading query bound to one specific target entity.
pub struct QueryRelationTo<R> {
    target: EntityId,
    phantom: PhantomData<R>,
}

phantom_debug!(QueryRelationTo<R> { target });
phantom_copy!(QueryRelationTo<R>);

impl<R> QueryRelationTo<R> {
    /// Returns relation query bound to one specific target entity.
    pub fn new(target: EntityId) -> Self {
        QueryRelationTo {
            target,
            phantom: PhantomData,
        }
    }
}

/// Fetch for the `Related<R>` query.
#[allow(missing_debug_implementations)]
pub struct FetchRelationToRead<R: Relation> {
    target: EntityId,
    item_idx: usize,
    ptr: NonNull<OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelationToRead<R>
where
    R: Relation,
{
    type Item = &'a R;

    #[inline]
    fn dangling() -> Self {
        FetchRelationToRead {
            target: EntityId::dangling(),
            ptr: NonNull::dangling(),
            item_idx: 0,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let origin_component = &*self.ptr.as_ptr().add(idx);
        let item_idx = origin_component
            .origins()
            .iter()
            .position(|origin| origin.target == self.target);

        match item_idx {
            None => true,
            Some(item_idx) => {
                self.item_idx = item_idx;
                false
            }
        }
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a R {
        let origin_component = &*self.ptr.as_ptr().add(idx);
        &origin_component.origins()[self.item_idx].relation
    }
}

unsafe impl<R> Query for QueryRelationTo<&R>
where
    R: Relation,
{
    type Fetch = FetchRelationToRead<R>;

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn conflicts<Q>(&self, query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(
            query.access(TypeId::of::<OriginComponent<R>>()),
            Some(Access::Write)
        )
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch(&mut self, archetype: &Archetype, _epoch: u64) -> FetchRelationToRead<R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<OriginComponent<R>>());

        FetchRelationToRead {
            target: self.target,
            ptr: data.ptr.cast(),
            item_idx: 0,
        }
    }
}

unsafe impl<R> ImmutableQuery for QueryRelationTo<&R> where R: Relation {}

/// Returns relation reading query bound to one specific target entity.
///
/// To get relation without specific entity use [`read_relation`].
pub fn read_relation_to<'a, R>(target: EntityId) -> QueryRelationTo<&'a R>
where
    R: Relation,
{
    QueryRelationTo {
        target,
        phantom: PhantomData,
    }
}

/// Fetch for the `Related<R>` query.
#[allow(missing_debug_implementations)]
pub struct FetchRelationToWrite<R: Relation> {
    target: EntityId,
    item_idx: usize,
    epoch: u64,
    ptr: NonNull<OriginComponent<R>>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelationToWrite<R>
where
    R: Relation,
{
    type Item = &'a R;

    #[inline]
    fn dangling() -> Self {
        FetchRelationToWrite {
            item_idx: 0,
            target: EntityId::dangling(),
            epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx);

        debug_assert!(*chunk_version < self.epoch);
        *chunk_version = self.epoch;
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let origin_component = &*self.ptr.as_ptr().add(idx);
        let item_idx = origin_component
            .origins()
            .iter()
            .position(|origin| origin.target == self.target);

        match item_idx {
            None => true,
            Some(item_idx) => {
                self.item_idx = item_idx;
                false
            }
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a R {
        let entity_version = &mut *self.entity_versions.as_ptr().add(idx);

        debug_assert!(*entity_version < self.epoch);
        *entity_version = self.epoch;

        let origin_component = &*self.ptr.as_ptr().add(idx);
        &origin_component.origins()[self.item_idx].relation
    }
}

unsafe impl<R> Query for QueryRelationTo<&mut R>
where
    R: Relation,
{
    type Fetch = FetchRelationToWrite<R>;

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn conflicts<Q>(&self, query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(
            query.access(TypeId::of::<OriginComponent<R>>()),
            Some(Access::Read | Access::Write)
        )
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch(&mut self, archetype: &Archetype, epoch: u64) -> FetchRelationToWrite<R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<OriginComponent<R>>());

        debug_assert!(*data.version.get() < epoch);
        *data.version.get() = epoch;

        FetchRelationToWrite {
            target: self.target,
            item_idx: 0,
            epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions,
        }
    }
}

/// Returns relation writing query bound to one specific target entity.
///
/// To get relation without specific entity use [`write_relation`].
pub fn write_relation_to<'a, R>(target: EntityId) -> QueryRelationTo<&'a mut R>
where
    R: Relation,
{
    QueryRelationTo {
        target,
        phantom: PhantomData,
    }
}

/// Fetch for the `Related<R>` query.
#[allow(missing_debug_implementations)]
pub struct FilterFetchRelationTo<R: Relation> {
    target: EntityId,
    ptr: NonNull<OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FilterFetchRelationTo<R>
where
    R: Relation,
{
    type Item = ();

    #[inline]
    fn dangling() -> Self {
        FilterFetchRelationTo {
            target: EntityId::dangling(),
            ptr: NonNull::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let origin_component = &*self.ptr.as_ptr().add(idx);
        origin_component
            .origins()
            .iter()
            .all(|origin| origin.target != self.target)
    }

    #[inline]
    unsafe fn get_item(&mut self, _: usize) -> () {}
}

/// Returns relation reading query bound to one specific target entity.
pub struct FilterRelationTo<R> {
    target: EntityId,
    phantom: PhantomData<R>,
}

phantom_debug!(FilterRelationTo<R> { target });

unsafe impl<R> Query for FilterRelationTo<R>
where
    R: Relation,
{
    type Fetch = FilterFetchRelationTo<R>;

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn conflicts<Q>(&self, query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(
            query.access(TypeId::of::<OriginComponent<R>>()),
            Some(Access::Write)
        )
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch(&mut self, archetype: &Archetype, _epoch: u64) -> FilterFetchRelationTo<R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<OriginComponent<R>>());

        FilterFetchRelationTo {
            target: self.target,
            ptr: data.ptr.cast(),
        }
    }
}

unsafe impl<R> ImmutableQuery for FilterRelationTo<R> where R: Relation {}

/// Returns relation filter bound to one specific target entity.
pub fn related_to<R>(target: EntityId) -> FilterRelationTo<R>
where
    R: Relation,
{
    FilterRelationTo {
        target,
        phantom: PhantomData,
    }
}
