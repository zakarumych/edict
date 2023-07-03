use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, Read, Write},
    relation::{OriginComponent, Relation},
};

marker_type! {
    /// Query for origins of relation.
    ///
    /// Yields relation instance and target.
    pub struct RelatesExclusive<R>;
}

/// Fetch for the [`RelatesExclusive<&R>`] query.
pub struct FetchRelatesExclusiveRead<'a, R: Relation> {
    ptr: NonNull<OriginComponent<R>>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesExclusiveRead<'a, R>
where
    R: Relation + Sync,
{
    type Item = (&'a R, EntityId);

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatesExclusiveRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> (&'a R, EntityId) {
        let origin_component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        let origin = &origin_component.relations()[0];
        (&origin.relation, origin.target)
    }
}

impl<R> IntoQuery for RelatesExclusive<&R>
where
    R: Relation + Sync,
{
    type Query = RelatesExclusive<Read<R>>;

    #[inline(always)]
    fn into_query(self) -> RelatesExclusive<Read<R>> {
        RelatesExclusive
    }
}

impl<R> DefaultQuery for RelatesExclusive<&R>
where
    R: Relation + Sync,
{
    #[inline(always)]
    fn default_query() -> RelatesExclusive<Read<R>> {
        RelatesExclusive
    }
}

impl<R> IntoQuery for RelatesExclusive<Read<R>>
where
    R: Relation + Sync,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for RelatesExclusive<Read<R>>
where
    R: Relation + Sync,
{
    #[inline(always)]
    fn default_query() -> Self {
        RelatesExclusive
    }
}

unsafe impl<R> Query for RelatesExclusive<Read<R>>
where
    R: Relation + Sync,
{
    type Item<'a> = (&'a R, EntityId);
    type Fetch<'a> = FetchRelatesExclusiveRead<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
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
    ) -> FetchRelatesExclusiveRead<'a, R> {
        assert!(
            R::EXCLUSIVE,
            "QueryExclusiveRelation can be used only with EXCLUSIVE relations"
        );

        let component = unsafe {
            archetype
                .component(TypeId::of::<OriginComponent<R>>())
                .unwrap_unchecked()
        };

        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = unsafe { component.data() };

        FetchRelatesExclusiveRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for RelatesExclusive<Read<R>> where R: Relation + Sync {}

/// Fetch for the [`RelatesExclusive<&mut R>`] query.
pub struct FetchRelatesExclusiveWrite<'a, R: Relation> {
    epoch: EpochId,
    ptr: NonNull<OriginComponent<R>>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesExclusiveWrite<'a, R>
where
    R: Relation + Send,
{
    type Item = (&'a mut R, EntityId);

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatesExclusiveWrite {
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
    unsafe fn get_item(&mut self, idx: u32) -> (&'a mut R, EntityId) {
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
        entity_epoch.bump(self.epoch);

        let origin_component = unsafe { &mut *self.ptr.as_ptr().add(idx as usize) };
        let origin = &mut origin_component.relations_mut()[0];
        (&mut origin.relation, origin.target)
    }
}

impl<R> IntoQuery for RelatesExclusive<&mut R>
where
    R: Relation + Send,
{
    type Query = RelatesExclusive<Write<R>>;

    #[inline(always)]
    fn into_query(self) -> RelatesExclusive<Write<R>> {
        RelatesExclusive
    }
}

impl<R> DefaultQuery for RelatesExclusive<&mut R>
where
    R: Relation + Send,
{
    #[inline(always)]
    fn default_query() -> RelatesExclusive<Write<R>> {
        RelatesExclusive
    }
}

impl<R> IntoQuery for RelatesExclusive<Write<R>>
where
    R: Relation + Send,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for RelatesExclusive<Write<R>>
where
    R: Relation + Send,
{
    #[inline(always)]
    fn default_query() -> Self {
        RelatesExclusive
    }
}

unsafe impl<R> Query for RelatesExclusive<Write<R>>
where
    R: Relation + Send,
{
    type Item<'a> = (&'a mut R, EntityId);
    type Fetch<'a> = FetchRelatesExclusiveWrite<'a, R>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Write)
        } else {
            None
        }
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
    ) -> FetchRelatesExclusiveWrite<'a, R> {
        assert!(
            R::EXCLUSIVE,
            "QueryExclusiveRelation can be used only with EXCLUSIVE relations"
        );

        let component = unsafe {
            archetype
                .component(TypeId::of::<OriginComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = unsafe { component.data_mut() };
        data.epoch.bump(epoch);

        FetchRelatesExclusiveWrite {
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
            chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
            marker: PhantomData,
        }
    }
}
