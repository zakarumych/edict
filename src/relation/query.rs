use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::{AtomicBorrow, AtomicBorrowMut};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    query::{
        Access, Fetch, ImmutablePhantomQuery, ImmutableQuery, PhantomQuery, PhantomQueryFetch,
        Query, QueryFetch,
    },
    relation::{Origin, OriginComponent, Relation},
};

use super::TargetComponent;

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
pub struct FetchRelationRead<'a, R: Relation> {
    ptr: NonNull<OriginComponent<R>>,
    _borrow: AtomicBorrow<'a>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelationRead<'a, R>
where
    R: Relation,
{
    type Item = RelationReadIter<'a, R>;

    #[inline]
    fn dangling() -> Self {
        FetchRelationRead {
            ptr: NonNull::dangling(),
            _borrow: AtomicBorrow::dummy(),
            marker: PhantomData,
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

impl<'a, R> PhantomQueryFetch<'a> for QueryRelation<&R>
where
    R: Relation,
{
    type Item = RelationReadIter<'a, R>;
    type Fetch = FetchRelationRead<'a, R>;
}

unsafe impl<R> PhantomQuery for QueryRelation<&R>
where
    R: Relation,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn access_any() -> Option<Access> {
        Some(Access::Read)
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

    #[inline]
    fn is_valid() -> bool {
        true
    }

    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: u64) -> FetchRelationRead<'a, R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);

        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FetchRelationRead {
            ptr: data.ptr.cast(),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutablePhantomQuery for QueryRelation<&R> where R: Relation {}

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
pub struct FetchRelationWrite<'a, R: Relation> {
    epoch: u64,
    ptr: NonNull<OriginComponent<R>>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
    _borrow: AtomicBorrowMut<'a>,
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelationWrite<'a, R>
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
            _borrow: AtomicBorrowMut::dummy(),
            marker: PhantomData,
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

impl<'a, R> PhantomQueryFetch<'a> for QueryRelation<&mut R>
where
    R: Relation,
{
    type Item = RelationWriteIter<'a, R>;
    type Fetch = FetchRelationWrite<'a, R>;
}

unsafe impl<R> PhantomQuery for QueryRelation<&mut R>
where
    R: Relation,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn access_any() -> Option<Access> {
        Some(Access::Write)
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

    #[inline]
    fn is_valid() -> bool {
        true
    }

    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: u64) -> FetchRelationWrite<'a, R> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let mut data = component.data.borrow_mut();

        debug_assert!(data.version < epoch);
        data.version = epoch;

        let (data, borrow) = atomicell::RefMut::into_split(data);

        FetchRelationWrite {
            epoch,
            ptr: data.ptr.cast(),
            entity_versions: NonNull::from(data.entity_versions.get_unchecked_mut(0)),
            chunk_versions: NonNull::from(data.chunk_versions.get_unchecked_mut(0)),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
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
pub struct FetchRelationToRead<'a, R: Relation> {
    target: EntityId,
    item_idx: usize,
    ptr: NonNull<OriginComponent<R>>,
    _borrow: AtomicBorrow<'a>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelationToRead<'a, R>
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
            _borrow: AtomicBorrow::dummy(),
            marker: PhantomData,
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

impl<'a, R> QueryFetch<'a> for QueryRelationTo<&R>
where
    R: Relation,
{
    type Item = &'a R;
    type Fetch = FetchRelationToRead<'a, R>;
}

unsafe impl<R> Query for QueryRelationTo<&R>
where
    R: Relation,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        Some(Access::Read)
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

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: u64,
    ) -> FetchRelationToRead<'a, R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FetchRelationToRead {
            target: self.target,
            ptr: data.ptr.cast(),
            item_idx: 0,
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for QueryRelationTo<&R> where R: Relation {}

/// Fetch for the `Related<R>` query.
#[allow(missing_debug_implementations)]
pub struct FetchRelationToWrite<'a, R: Relation> {
    target: EntityId,
    item_idx: usize,
    epoch: u64,
    ptr: NonNull<OriginComponent<R>>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
    _borrow: AtomicBorrowMut<'a>,
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelationToWrite<'a, R>
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
            _borrow: AtomicBorrowMut::dummy(),
            marker: PhantomData,
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

impl<'a, R> QueryFetch<'a> for QueryRelationTo<&mut R>
where
    R: Relation,
{
    type Item = &'a R;
    type Fetch = FetchRelationToWrite<'a, R>;
}

unsafe impl<R> Query for QueryRelationTo<&mut R>
where
    R: Relation,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        Some(Access::Write)
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

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: u64,
    ) -> FetchRelationToWrite<'a, R> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let mut data = component.data.borrow_mut();

        debug_assert!(data.version < epoch);
        data.version = epoch;

        let (data, borrow) = atomicell::RefMut::into_split(data);

        FetchRelationToWrite {
            target: self.target,
            item_idx: 0,
            epoch,
            ptr: data.ptr.cast(),
            entity_versions: NonNull::from(data.entity_versions.get_unchecked_mut(0)),
            chunk_versions: NonNull::from(data.chunk_versions.get_unchecked_mut(0)),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

/// Fetch for the `Related<R>` query.
#[allow(missing_debug_implementations)]
pub struct FilterFetchRelationTo<'a, R: Relation> {
    target: EntityId,
    ptr: NonNull<OriginComponent<R>>,
    _borrow: AtomicBorrow<'a>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FilterFetchRelationTo<'a, R>
where
    R: Relation,
{
    type Item = ();

    #[inline]
    fn dangling() -> Self {
        FilterFetchRelationTo {
            target: EntityId::dangling(),
            ptr: NonNull::dangling(),
            _borrow: AtomicBorrow::dummy(),
            marker: PhantomData,
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
pub struct WithRelationTo<R> {
    target: EntityId,
    phantom: PhantomData<R>,
}

phantom_debug!(WithRelationTo<R> { target });

impl<R> WithRelationTo<R> {
    /// Returns relation filter bound to one specific target entity.
    pub const fn new(target: EntityId) -> Self {
        WithRelationTo {
            target,
            phantom: PhantomData,
        }
    }
}

impl<'a, R> QueryFetch<'a> for WithRelationTo<R>
where
    R: Relation,
{
    type Item = ();
    type Fetch = FilterFetchRelationTo<'a, R>;
}

unsafe impl<R> Query for WithRelationTo<R>
where
    R: Relation,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        Some(Access::Read)
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

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: u64,
    ) -> FilterFetchRelationTo<'a, R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FilterFetchRelationTo {
            target: self.target,
            ptr: data.ptr.cast(),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for WithRelationTo<R> where R: Relation {}

phantom_newtype! {
    /// Query that yields targets of relations of type `R`.
    /// For each target it yields slice of entity ids related to the target.
    pub struct QueryRelated<R>
}

/// Fetch type for [`Related<R>`]
#[allow(missing_debug_implementations)]
pub struct FetchRelated<'a, R> {
    ptr: NonNull<TargetComponent<R>>,
    _borrow: AtomicBorrow<'a>,
    marker: PhantomData<&'a TargetComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelated<'a, R>
where
    R: Relation,
{
    type Item = &'a [EntityId];

    #[inline]
    fn dangling() -> Self {
        FetchRelated {
            ptr: NonNull::dangling(),
            _borrow: AtomicBorrow::dummy(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a [EntityId] {
        let component = &*self.ptr.as_ptr().add(idx);
        &component.origins[..]
    }
}

impl<'a, R> PhantomQueryFetch<'a> for QueryRelated<R>
where
    R: Relation,
{
    type Item = &'a [EntityId];
    type Fetch = FetchRelated<'a, R>;
}

unsafe impl<R> PhantomQuery for QueryRelated<R>
where
    R: Relation,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<TargetComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn access_any() -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn conflicts<Q>(query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(
            query.access(TypeId::of::<TargetComponent<R>>()),
            Some(Access::Write)
        )
    }

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<TargetComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: u64) -> FetchRelated<'a, R> {
        let idx = archetype
            .id_index(TypeId::of::<TargetComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FetchRelated {
            ptr: data.ptr.cast(),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutablePhantomQuery for QueryRelated<R> where R: Relation {}

/// Returns relation query not bound to any target entity.
/// Yields all relations of a given type on entities.
///
/// To get relation with specific entity use [`relation_to`].
pub fn relation<R>() -> PhantomData<QueryRelation<R>>
where
    R: Relation,
{
    PhantomData
}

/// Returns relation query bound to one specific target entity.
///
/// To get relation without specific entity use [`relation`].
pub fn relation_to<R>(target: EntityId) -> QueryRelationTo<R> {
    QueryRelationTo {
        target,
        phantom: PhantomData,
    }
}

/// Returns relation filter bound to one specific target entity.
pub fn with_relation_to<R>(target: EntityId) -> WithRelationTo<R>
where
    R: Relation,
{
    WithRelationTo {
        target,
        phantom: PhantomData,
    }
}

/// Returns query that yields targets of relations of type `R`.
/// For each target it yields slice of entity ids related to the target.
pub fn related<R>() -> QueryRelated<R>
where
    R: Relation,
{
    QueryRelated::new()
}
