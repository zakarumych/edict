use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    entity::{EntityBound, EntityId},
    epoch::EpochId,
    query::{
        AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, Read, SendQuery, Write,
        WriteAlias,
    },
    relation::{ExclusiveRelation, OriginComponent},
    system::QueryArg,
    type_id, Access,
};

marker_type! {
    /// Query for origins of relation.
    ///
    /// Yields relation instance and target.
    pub struct RelatesExclusive<R>;
}

/// Fetch for the [`RelatesExclusive<&R>`] query.
pub struct FetchRelatesExclusiveRead<'a, R: ExclusiveRelation> {
    ptr: NonNull<OriginComponent<R>>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesExclusiveRead<'a, R>
where
    R: ExclusiveRelation,
{
    type Item = (&'a R, EntityBound<'a>);

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatesExclusiveRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> (&'a R, EntityBound<'a>) {
        let origin_component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        let origin = &origin_component.relations()[0];
        (&origin.relation, EntityBound::new(origin.target))
    }
}

impl<R> AsQuery for RelatesExclusive<&R>
where
    R: ExclusiveRelation,
{
    type Query = RelatesExclusive<Read<R>>;
}

impl<R> DefaultQuery for RelatesExclusive<&R>
where
    R: ExclusiveRelation,
{
    #[inline(always)]
    fn default_query() -> RelatesExclusive<Read<R>> {
        RelatesExclusive
    }
}

impl<R> AsQuery for RelatesExclusive<Read<R>>
where
    R: ExclusiveRelation,
{
    type Query = Self;
}

impl<R> IntoQuery for RelatesExclusive<Read<R>>
where
    R: ExclusiveRelation,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for RelatesExclusive<Read<R>>
where
    R: ExclusiveRelation,
{
    #[inline(always)]
    fn default_query() -> Self {
        RelatesExclusive
    }
}

impl<R> QueryArg for RelatesExclusive<Read<R>>
where
    R: Sync + ExclusiveRelation,
{
    #[inline(always)]
    fn new() -> Self {
        RelatesExclusive
    }
}

unsafe impl<R> Query for RelatesExclusive<Read<R>>
where
    R: ExclusiveRelation,
{
    type Item<'a> = (&'a R, EntityBound<'a>);
    type Fetch<'a> = FetchRelatesExclusiveRead<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<OriginComponent<R>>() {
            Ok(Some(Access::Read))
        } else {
            Ok(None)
        }
    }

    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(type_id::<OriginComponent<R>>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(type_id::<OriginComponent<R>>(), Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelatesExclusiveRead<'a, R> {
        let () = R::ASSERT_EXCLUSIVE;

        let component = unsafe {
            archetype
                .component(type_id::<OriginComponent<R>>())
                .unwrap_unchecked()
        };

        debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

        let data = unsafe { component.data() };

        FetchRelatesExclusiveRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for RelatesExclusive<Read<R>> where R: ExclusiveRelation {}
unsafe impl<R> SendQuery for RelatesExclusive<Read<R>> where R: ExclusiveRelation + Sync {}

/// Fetch for the [`RelatesExclusive<&mut R>`] query.
pub struct FetchRelatesExclusiveWrite<'a, R: ExclusiveRelation> {
    epoch: EpochId,
    ptr: NonNull<OriginComponent<R>>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesExclusiveWrite<'a, R>
where
    R: ExclusiveRelation,
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

impl<R> AsQuery for RelatesExclusive<&mut R>
where
    R: ExclusiveRelation,
{
    type Query = RelatesExclusive<Write<R>>;
}

impl<R> DefaultQuery for RelatesExclusive<&mut R>
where
    R: ExclusiveRelation,
{
    #[inline(always)]
    fn default_query() -> RelatesExclusive<Write<R>> {
        RelatesExclusive
    }
}

impl<R> AsQuery for RelatesExclusive<Write<R>>
where
    R: ExclusiveRelation,
{
    type Query = Self;
}

impl<R> IntoQuery for RelatesExclusive<Write<R>>
where
    R: ExclusiveRelation,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for RelatesExclusive<Write<R>>
where
    R: ExclusiveRelation,
{
    #[inline(always)]
    fn default_query() -> Self {
        RelatesExclusive
    }
}

impl<R> QueryArg for RelatesExclusive<Write<R>>
where
    R: Send + ExclusiveRelation,
{
    #[inline(always)]
    fn new() -> Self {
        RelatesExclusive
    }
}

unsafe impl<R> Query for RelatesExclusive<Write<R>>
where
    R: ExclusiveRelation,
{
    type Item<'a> = (&'a mut R, EntityId);
    type Fetch<'a> = FetchRelatesExclusiveWrite<'a, R>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<OriginComponent<R>>() {
            Ok(Some(Access::Write))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(type_id::<OriginComponent<R>>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(type_id::<OriginComponent<R>>(), Access::Write)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchRelatesExclusiveWrite<'a, R> {
        let () = R::ASSERT_EXCLUSIVE;

        let component = unsafe {
            archetype
                .component(type_id::<OriginComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

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

unsafe impl<R> SendQuery for RelatesExclusive<Write<R>> where R: ExclusiveRelation + Send {}
