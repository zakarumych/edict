use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query},
    relation::{OriginComponent, Relation},
};

/// Query for origins of relation with specified target.
///
/// Yields relation instance.
pub struct RelatesTo<R> {
    target: EntityId,
    phantom: PhantomData<R>,
}

impl_debug!(RelatesTo<R> { target });
impl_copy!(RelatesTo<R>);

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
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let origin_component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        let item_idx = origin_component
            .relations()
            .iter()
            .position(|origin| origin.target == self.target);

        match item_idx {
            None => false,
            Some(item_idx) => {
                self.item_idx = item_idx;
                true
            }
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> &'a R {
        let origin_component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        &origin_component.relations()[self.item_idx].relation
    }
}

impl<R> IntoQuery for RelatesTo<&R>
where
    R: Relation + 'static,
{
    type Query = Self;

    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<R> Query for RelatesTo<&R>
where
    R: Relation + Sync,
{
    type Item<'a> = &'a R;
    type Fetch<'a> = FetchRelatesToRead<'a, R>;

    const MUTABLE: bool = false;
    const FILTERS_ENTITIES: bool = true;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<OriginComponent<R>>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelatesToRead<'a, R> {
        let component = unsafe {
            archetype
                .component(TypeId::of::<OriginComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = unsafe { component.data() };

        FetchRelatesToRead {
            target: self.target,
            ptr: data.ptr.cast(),
            item_idx: 0,
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
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesToWrite<'a, R>
where
    R: Relation + Send,
{
    type Item = &'a mut R;

    #[inline]
    fn dangling() -> Self {
        FetchRelatesToWrite {
            item_idx: 0,
            target: EntityId::dangling(),
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        chunk_epoch.bump(self.epoch);
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let origin_component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        let item_idx = origin_component
            .relations()
            .iter()
            .position(|origin| origin.target == self.target);

        match item_idx {
            None => false,
            Some(item_idx) => {
                self.item_idx = item_idx;
                true
            }
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> &'a mut R {
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
        entity_epoch.bump(self.epoch);

        let origin_component = unsafe { &mut *self.ptr.as_ptr().add(idx as usize) };
        &mut origin_component.relations_mut()[self.item_idx].relation
    }
}

impl<R> IntoQuery for RelatesTo<&mut R>
where
    R: Relation + Send,
{
    type Query = Self;

    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<R> Query for RelatesTo<&mut R>
where
    R: Relation + Send,
{
    type Item<'a> = &'a mut R;
    type Fetch<'a> = FetchRelatesToWrite<'a, R>;

    const MUTABLE: bool = true;
    const FILTERS_ENTITIES: bool = true;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<OriginComponent<R>>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchRelatesToWrite<'a, R> {
        let component = unsafe {
            archetype
                .component(TypeId::of::<OriginComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = unsafe { component.data_mut() };
        data.epoch.bump(epoch);

        FetchRelatesToWrite {
            target: self.target,
            item_idx: 0,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
            chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
            marker: PhantomData,
        }
    }
}
