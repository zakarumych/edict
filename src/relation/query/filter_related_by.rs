use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query},
    relation::{Relation, TargetComponent},
};

/// Fetch for the `FilterRelatedBy<R>` query.
pub struct FetchFilterRelatedBy<'a, R: Relation> {
    origin: EntityId,
    ptr: NonNull<TargetComponent<R>>,
    marker: PhantomData<&'a TargetComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchFilterRelatedBy<'a, R>
where
    R: Relation,
{
    type Item = ();

    #[inline]
    fn dangling() -> Self {
        FetchFilterRelatedBy {
            origin: EntityId::dangling(),
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        let target_component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        target_component.origins.contains(&self.origin)
    }

    #[inline]
    unsafe fn get_item(&mut self, _: u32) -> () {}
}

/// Filters targets of relation with specified origin.
pub struct FilterRelatedBy<R> {
    origin: EntityId,
    phantom: PhantomData<R>,
}

impl_debug!(FilterRelatedBy<R> { origin });
impl_copy!(FilterRelatedBy<R>);

impl<R> FilterRelatedBy<R> {
    /// Returns relation filter bound to one specific origin.
    pub const fn new(origin: EntityId) -> Self {
        FilterRelatedBy {
            origin,
            phantom: PhantomData,
        }
    }
}

impl<R> IntoQuery for FilterRelatedBy<R>
where
    R: Relation,
{
    type Query = Self;

    #[inline]
    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<R> Query for FilterRelatedBy<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = FetchFilterRelatedBy<'a, R>;

    const MUTABLE: bool = false;
    const FILTERS_ENTITIES: bool = true;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<TargetComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<TargetComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<TargetComponent<R>>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchFilterRelatedBy<'a, R> {
        let component = unsafe {
            archetype
                .component(TypeId::of::<TargetComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

        let data = unsafe { component.data() };

        FetchFilterRelatedBy {
            origin: self.origin,
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for FilterRelatedBy<R> where R: Relation {}

/// Returns a filter to filter targets of relation with specified origin.
pub fn related_by<R: Relation>(origin: EntityId) -> FilterRelatedBy<R> {
    FilterRelatedBy {
        origin,
        phantom: PhantomData,
    }
}
