use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::{AtomicBorrow, AtomicBorrowMut};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query, QueryFetch},
    relation::{OriginComponent, Relation},
};

/// Query for origins of relation with specified target.
///
/// Yields relation instance.
pub struct RelatesTo<R> {
    target: EntityId,
    phantom: PhantomData<R>,
}

phantom_debug!(RelatesTo<R> { target });
phantom_copy!(RelatesTo<R>);

impl<R> RelatesTo<R> {
    /// Returns relation query bound to one specific target entity.
    pub fn new(target: EntityId) -> Self {
        RelatesTo {
            target,
            phantom: PhantomData,
        }
    }
}

/// Fetch for the [`RelatesTo<R>`] query.
pub struct FetchRelatesToRead<'a, R: Relation> {
    target: EntityId,
    item_idx: usize,
    ptr: NonNull<OriginComponent<R>>,
    _borrow: AtomicBorrow<'a>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesToRead<'a, R>
where
    R: Relation + Sync,
{
    type Item = &'a R;

    #[inline]
    fn dangling() -> Self {
        FetchRelatesToRead {
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

impl<'a, R> QueryFetch<'a> for RelatesTo<&R>
where
    R: Relation + Sync,
{
    type Item = &'a R;
    type Fetch = FetchRelatesToRead<'a, R>;
}

impl<R> IntoQuery for RelatesTo<&R>
where
    R: Relation + 'static,
{
    type Query = Self;
}

impl<R> Query for RelatesTo<&R>
where
    R: Relation + Sync,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelatesToRead<'a, R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FetchRelatesToRead {
            target: self.target,
            ptr: data.ptr.cast(),
            item_idx: 0,
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for RelatesTo<&R> where R: Relation + Sync {}

/// Fetch for the `RelatesTo<R>` query.
pub struct FetchRelatesToWrite<'a, R: Relation> {
    target: EntityId,
    item_idx: usize,
    epoch: EpochId,
    ptr: NonNull<OriginComponent<R>>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    _borrow: AtomicBorrowMut<'a>,
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesToWrite<'a, R>
where
    R: Relation + Send,
{
    type Item = &'a R;

    #[inline]
    fn dangling() -> Self {
        FetchRelatesToWrite {
            item_idx: 0,
            target: EntityId::dangling(),
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
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_epoch.bump(self.epoch);
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
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx);
        entity_epoch.bump(self.epoch);

        let origin_component = &*self.ptr.as_ptr().add(idx);
        &origin_component.origins()[self.item_idx].relation
    }
}

impl<'a, R> QueryFetch<'a> for RelatesTo<&mut R>
where
    R: Relation + Send,
{
    type Item = &'a R;
    type Fetch = FetchRelatesToWrite<'a, R>;
}

impl<R> IntoQuery for RelatesTo<&mut R>
where
    R: Relation + Send,
{
    type Query = Self;
}

impl<R> Query for RelatesTo<&mut R>
where
    R: Relation + Send,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchRelatesToWrite<'a, R> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let mut data = component.data.borrow_mut();
        data.epoch.bump(epoch);

        let (data, borrow) = atomicell::RefMut::into_split(data);

        FetchRelatesToWrite {
            target: self.target,
            item_idx: 0,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}
