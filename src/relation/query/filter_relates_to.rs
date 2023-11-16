use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{AsQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias},
    relation::{OriginComponent, Relation},
    Access,
};

/// Fetch for the `RelatesTo<R>` query.
pub struct FilterFetchRelatesTo<'a, R: Relation> {
    target: EntityId,
    ptr: NonNull<OriginComponent<R>>,
    marker: PhantomData<&'a OriginComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FilterFetchRelatesTo<'a, R>
where
    R: Relation,
{
    type Item = ();

    #[inline(always)]
    fn dangling() -> Self {
        FilterFetchRelatesTo {
            target: EntityId::dangling(),
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let origin_component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        origin_component
            .relations()
            .iter()
            .any(|origin| origin.target == self.target)
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, _: u32) -> () {}
}

/// Filters origins of relation with specified target.
pub struct FilterRelatesTo<R> {
    target: EntityId,
    phantom: PhantomData<fn() -> R>,
}

impl_debug!(FilterRelatesTo<R> { target });
impl_copy!(FilterRelatesTo<R>);

impl<R> FilterRelatesTo<R> {
    /// Returns relation filter bound to one specific target.
    pub const fn new(target: EntityId) -> Self {
        FilterRelatesTo {
            target,
            phantom: PhantomData,
        }
    }
}

impl<R> AsQuery for FilterRelatesTo<R>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for FilterRelatesTo<R>
where
    R: Relation,
{
    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<R> Query for FilterRelatesTo<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = FilterFetchRelatesTo<'a, R>;

    const MUTABLE: bool = false;
    const FILTERS_ENTITIES: bool = true;

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
    ) -> FilterFetchRelatesTo<'a, R> {
        let component = unsafe {
            archetype
                .component(TypeId::of::<OriginComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = unsafe { component.data() };

        FilterFetchRelatesTo {
            target: self.target,
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for FilterRelatesTo<R> where R: Relation {}
unsafe impl<R> SendQuery for FilterRelatesTo<R> where R: Relation {}
