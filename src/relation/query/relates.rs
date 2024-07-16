use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, Read, SendQuery, With,
        Write, WriteAlias,
    },
    relation::{OriginComponent, Relation},
    system::QueryArg,
    type_id, Access,
};

use super::{RelationIter, RelationReadIter, RelationWriteIter};

marker_type! {
    /// Query for origins of relation.
    ///
    /// Yields iterator of pairs - relation instance and target.
    pub struct Relates<R>;
}

impl<R> AsQuery for Relates<With<R>>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for Relates<With<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn into_query(self) -> Self::Query {
        self
    }
}

impl<R> DefaultQuery for Relates<With<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        Relates
    }
}

/// Fetch for the [`Relates<&R>`] query.
pub struct FetchRelatesWith<'a, R: Relation> {
    ptr: NonNull<OriginComponent<R>>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesWith<'a, R>
where
    R: Relation,
{
    type Item = RelationIter<'a, R>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatesWith {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> RelationIter<'a, R> {
        let component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };

        RelationIter::new(component.targets())
    }
}

unsafe impl<R> Query for Relates<With<R>>
where
    R: Relation,
{
    type Item<'a> = RelationIter<'a, R>;
    type Fetch<'a> = FetchRelatesWith<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<OriginComponent<R>>() {
            Ok(Some(Access::Read))
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
        f(type_id::<OriginComponent<R>>(), Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelatesWith<'a, R> {
        let component = unsafe {
            archetype
                .component(type_id::<OriginComponent<R>>())
                .unwrap_unchecked()
        };

        debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

        let data = unsafe { component.data() };

        FetchRelatesWith {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for Relates<With<R>> where R: Relation {}
unsafe impl<R> SendQuery for Relates<With<R>> where R: Relation {}

impl<R> QueryArg for Relates<With<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn new() -> Self {
        Relates
    }
}

impl<R> AsQuery for Relates<&R>
where
    R: Relation,
{
    type Query = Relates<Read<R>>;
}

impl<R> DefaultQuery for Relates<&R>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Relates<Read<R>> {
        Relates
    }
}

impl<R> AsQuery for Relates<Read<R>>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for Relates<Read<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn into_query(self) -> Self::Query {
        self
    }
}

impl<R> DefaultQuery for Relates<Read<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        Relates
    }
}

/// Fetch for the [`Relates<&R>`] query.
pub struct FetchRelatesRead<'a, R: Relation> {
    ptr: NonNull<OriginComponent<R>>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesRead<'a, R>
where
    R: Relation,
{
    type Item = RelationReadIter<'a, R>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatesRead {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> RelationReadIter<'a, R> {
        let component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };

        RelationReadIter::new(component.targets())
    }
}

unsafe impl<R> Query for Relates<Read<R>>
where
    R: Relation,
{
    type Item<'a> = RelationReadIter<'a, R>;
    type Fetch<'a> = FetchRelatesRead<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<OriginComponent<R>>() {
            Ok(Some(Access::Read))
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
        f(type_id::<OriginComponent<R>>(), Access::Read)
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
                .component(type_id::<OriginComponent<R>>())
                .unwrap_unchecked()
        };

        debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

        let data = unsafe { component.data() };

        FetchRelatesRead {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for Relates<Read<R>> where R: Relation {}
unsafe impl<R> SendQuery for Relates<Read<R>> where R: Relation + Sync {}

impl<R> QueryArg for Relates<Read<R>>
where
    R: Sync + Relation,
{
    #[inline(always)]
    fn new() -> Self {
        Relates
    }
}

impl<R> AsQuery for Relates<&mut R>
where
    R: Relation,
{
    type Query = Relates<Write<R>>;
}

impl<R> DefaultQuery for Relates<&mut R>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Relates<Write<R>> {
        Relates
    }
}

impl<R> AsQuery for Relates<Write<R>>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for Relates<Write<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for Relates<Write<R>>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        Relates
    }
}

/// Fetch for the [`Relates<&mut R>`] query.
pub struct FetchRelatesWrite<'a, R: Relation> {
    ptr: NonNull<OriginComponent<R>>,
    epoch: EpochId,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a mut OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelatesWrite<'a, R>
where
    R: Relation,
{
    type Item = RelationWriteIter<'a, R>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelatesWrite {
            ptr: NonNull::dangling(),
            epoch: EpochId::start(),
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
    unsafe fn get_item(&mut self, idx: u32) -> RelationWriteIter<'a, R> {
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };
        entity_epoch.bump(self.epoch);

        let component = unsafe { &mut *self.ptr.as_ptr().add(idx as usize) };

        RelationWriteIter::new(component.targets_mut())
    }
}

unsafe impl<R> Query for Relates<Write<R>>
where
    R: Relation,
{
    type Item<'a> = RelationWriteIter<'a, R>;
    type Fetch<'a> = FetchRelatesWrite<'a, R>;

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
    ) -> FetchRelatesWrite<'a, R> {
        let component = unsafe {
            archetype
                .component(type_id::<OriginComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

        let data = unsafe { component.data_mut() };
        data.epoch.bump(epoch);

        FetchRelatesWrite {
            ptr: data.ptr.cast(),
            epoch,
            entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()) },
            chunk_epochs: unsafe { NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()) },
            marker: PhantomData,
        }
    }
}

unsafe impl<R> SendQuery for Relates<Write<R>> where R: Relation + Send {}

impl<R> QueryArg for Relates<Write<R>>
where
    R: Send + Relation,
{
    #[inline(always)]
    fn new() -> Self {
        Relates
    }
}
