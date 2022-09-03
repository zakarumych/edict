use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::{AtomicBorrow, AtomicBorrowMut};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, IntoQuery, PhantomQuery, PhantomQueryFetch},
    relation::{OriginComponent, Relation},
};

phantom_newtype! {
    /// Query for origins of relation.
    ///
    /// Yields relation instance and target.
    pub struct RelatesExclusive<R>
}

/// Fetch for the [`RelatesExclusive<&R>`] query.
pub struct FetchRelatesExclusiveRead<'a, R: Relation> {
    ptr: NonNull<OriginComponent<R>>,
    _borrow: AtomicBorrow<'a>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesExclusiveRead<'a, R>
where
    R: Relation + Sync,
{
    type Item = (&'a R, EntityId);

    #[inline]
    fn dangling() -> Self {
        FetchRelatesExclusiveRead {
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
    unsafe fn get_item(&mut self, idx: usize) -> (&'a R, EntityId) {
        let origin_component = &*self.ptr.as_ptr().add(idx);
        let origin = &origin_component.origins()[0];
        (&origin.relation, origin.target)
    }
}

impl<'a, R> PhantomQueryFetch<'a> for RelatesExclusive<&R>
where
    R: Relation + Sync,
{
    type Item = (&'a R, EntityId);
    type Fetch = FetchRelatesExclusiveRead<'a, R>;
}

impl<R> IntoQuery for RelatesExclusive<&R>
where
    R: Relation + Sync,
{
    type Query = PhantomData<fn() -> Self>;
}

impl<R> PhantomQuery for RelatesExclusive<&R>
where
    R: Relation + Sync,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelatesExclusiveRead<'a, R> {
        assert!(
            R::EXCLUSIVE,
            "QueryExclusiveRelation can be used only with EXCLUSIVE relations"
        );

        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);

        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FetchRelatesExclusiveRead {
            ptr: data.ptr.cast(),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutablePhantomQuery for RelatesExclusive<&R> where R: Relation + Sync {}

/// Fetch for the [`RelatesExclusive<&mut R>`] query.
pub struct FetchRelatesExclusiveWrite<'a, R: Relation> {
    epoch: EpochId,
    ptr: NonNull<OriginComponent<R>>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    _borrow: AtomicBorrowMut<'a>,
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesExclusiveWrite<'a, R>
where
    R: Relation + Send,
{
    type Item = (&'a mut R, EntityId);

    #[inline]
    fn dangling() -> Self {
        FetchRelatesExclusiveWrite {
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
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
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_epoch.bump(self.epoch);
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> (&'a mut R, EntityId) {
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx);
        entity_epoch.bump(self.epoch);

        let origin_component = &mut *self.ptr.as_ptr().add(idx);
        let origin = &mut origin_component.origins_mut()[0];
        (&mut origin.relation, origin.target)
    }
}

impl<'a, R> PhantomQueryFetch<'a> for RelatesExclusive<&mut R>
where
    R: Relation + Send,
{
    type Item = (&'a mut R, EntityId);
    type Fetch = FetchRelatesExclusiveWrite<'a, R>;
}

impl<R> IntoQuery for RelatesExclusive<&mut R>
where
    R: Relation + 'static,
{
    type Query = PhantomData<fn() -> Self>;
}

impl<R> PhantomQuery for RelatesExclusive<&mut R>
where
    R: Relation + Send,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchRelatesExclusiveWrite<'a, R> {
        assert!(
            R::EXCLUSIVE,
            "QueryExclusiveRelation can be used only with EXCLUSIVE relations"
        );

        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let mut data = component.data.borrow_mut();
        data.epoch.bump(epoch);

        let (data, borrow) = atomicell::RefMut::into_split(data);

        FetchRelatesExclusiveWrite {
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}
