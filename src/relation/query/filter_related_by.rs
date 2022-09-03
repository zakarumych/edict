use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::AtomicBorrow;

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query, QueryFetch},
    relation::{Relation, TargetComponent},
};

/// Fetch for the `FilterRelatedBy<R>` query.
pub struct FetchFilterRelatedBy<'a, R: Relation> {
    origin: EntityId,
    ptr: NonNull<TargetComponent<R>>,
    _borrow: AtomicBorrow<'a>,
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
            _borrow: AtomicBorrow::dummy(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let target_component = &*self.ptr.as_ptr().add(idx);
        target_component.origins.contains(&self.origin)
    }

    #[inline]
    unsafe fn get_item(&mut self, _: usize) -> () {}
}

/// Filters targets of relation with specified origin.
pub struct FilterRelatedBy<R> {
    origin: EntityId,
    phantom: PhantomData<R>,
}

phantom_debug!(FilterRelatedBy<R> { origin });

impl<R> FilterRelatedBy<R> {
    /// Returns relation filter bound to one specific origin.
    pub const fn new(origin: EntityId) -> Self {
        FilterRelatedBy {
            origin,
            phantom: PhantomData,
        }
    }
}

impl<'a, R> QueryFetch<'a> for FilterRelatedBy<R>
where
    R: Relation,
{
    type Item = ();
    type Fetch = FetchFilterRelatedBy<'a, R>;
}

impl<R> IntoQuery for FilterRelatedBy<R>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> Query for FilterRelatedBy<R>
where
    R: Relation,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<TargetComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<TargetComponent<R>>())
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchFilterRelatedBy<'a, R> {
        let idx = archetype
            .id_index(TypeId::of::<TargetComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FetchFilterRelatedBy {
            origin: self.origin,
            ptr: data.ptr.cast(),
            _borrow: borrow,
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
