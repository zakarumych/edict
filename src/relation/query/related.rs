use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityBound,
    epoch::EpochId,
    query::{DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, WriteAlias},
    relation::{Relation, TargetComponent},
    system::QueryArg,
    Access,
};

marker_type! {
    /// Query for target of relation.
    ///
    /// Yields slices of origin ids for each target.
    pub struct Related<R>;
}

impl<R> IntoQuery for Related<R>
where
    R: Relation,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<R> DefaultQuery for Related<R>
where
    R: Relation,
{
    #[inline(always)]
    fn default_query() -> Self {
        Related
    }
}

impl<R> QueryArg for Related<R>
where
    R: Relation,
{
    #[inline(always)]
    fn new() -> Self {
        Related
    }
}

/// Fetch type for [`Related<R>`]
pub struct FetchRelated<'a, R> {
    ptr: NonNull<TargetComponent<R>>,
    marker: PhantomData<&'a TargetComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelated<'a, R>
where
    R: Relation,
{
    type Item = &'a [EntityBound<'a>];

    #[inline(always)]
    fn dangling() -> Self {
        FetchRelated {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> &'a [EntityBound<'a>] {
        let component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        EntityBound::wrap_slice(&component.origins[..])
    }
}

unsafe impl<R> Query for Related<R>
where
    R: Relation,
{
    type Item<'a> = &'a [EntityBound<'a>];
    type Fetch<'a> = FetchRelated<'a, R>;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn component_type_access(&self, ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        Ok(Access::read_type::<TargetComponent<R>>(ty))
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<TargetComponent<R>>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<TargetComponent<R>>(), Access::Read)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelated<'a, R> {
        let component = unsafe {
            archetype
                .component(TypeId::of::<TargetComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

        let data = unsafe { component.data() };

        FetchRelated {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for Related<R> where R: Relation {}
