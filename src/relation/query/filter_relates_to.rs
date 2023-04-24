use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query},
    relation::{OriginComponent, Relation},
};

/// Fetch for the `Related<R>` query.
pub struct FilterFetchRelationTo<'a, R: Relation> {
    target: EntityId,
    ptr: NonNull<OriginComponent<R>>,
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
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: usize) -> bool {
        let origin_component = &*self.ptr.as_ptr().add(idx);
        origin_component
            .origins()
            .iter()
            .any(|origin| origin.target == self.target)
    }

    #[inline]
    unsafe fn get_item(&mut self, _: usize) -> () {}
}

/// Filters origins of relation with specified target.
pub struct FilterRelatesTo<R> {
    target: EntityId,
    phantom: PhantomData<R>,
}

impl_debug!(FilterRelatesTo<R> { target });

impl<R> FilterRelatesTo<R> {
    /// Returns relation filter bound to one specific target.
    pub const fn new(target: EntityId) -> Self {
        FilterRelatesTo {
            target,
            phantom: PhantomData,
        }
    }
}

impl<R> IntoQuery for FilterRelatesTo<R>
where
    R: Relation,
{
    type Query = Self;

    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<R> Query for FilterRelatesTo<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = FilterFetchRelationTo<'a, R>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<OriginComponent<R>>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FilterFetchRelationTo<'a, R> {
        let component = archetype
            .component(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let data = component.data();

        FilterFetchRelationTo {
            target: self.target,
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutableQuery for FilterRelatesTo<R> where R: Relation {}

/// Returns a filter to filter origins of relation with specified target.
pub fn relates_to<R: Relation>(target: EntityId) -> FilterRelatesTo<R> {
    FilterRelatesTo {
        target,
        phantom: PhantomData,
    }
}
