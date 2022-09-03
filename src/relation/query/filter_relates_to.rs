use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::AtomicBorrow;

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query, QueryFetch},
    relation::{OriginComponent, Relation},
};

/// Fetch for the `Related<R>` query.
pub struct FilterFetchRelationTo<'a, R: Relation> {
    target: EntityId,
    ptr: NonNull<OriginComponent<R>>,
    _borrow: AtomicBorrow<'a>,
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
        let origin_component = &*self.ptr.as_ptr().add(idx);
        origin_component
            .origins()
            .iter()
            .all(|origin| origin.target != self.target)
    }

    #[inline]
    unsafe fn get_item(&mut self, _: usize) -> () {}
}

/// Filters origins of relation with specified target.
pub struct FilterRelatesTo<R> {
    target: EntityId,
    phantom: PhantomData<R>,
}

phantom_debug!(FilterRelatesTo<R> { target });

impl<R> FilterRelatesTo<R> {
    /// Returns relation filter bound to one specific target.
    pub const fn new(target: EntityId) -> Self {
        FilterRelatesTo {
            target,
            phantom: PhantomData,
        }
    }
}

impl<'a, R> QueryFetch<'a> for FilterRelatesTo<R>
where
    R: Relation,
{
    type Item = ();
    type Fetch = FilterFetchRelationTo<'a, R>;
}

impl<R> IntoQuery for FilterRelatesTo<R>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> Query for FilterRelatesTo<R>
where
    R: Relation,
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
    ) -> FilterFetchRelationTo<'a, R> {
        let idx = archetype
            .id_index(TypeId::of::<OriginComponent<R>>())
            .unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        FilterFetchRelationTo {
            target: self.target,
            ptr: data.ptr.cast(),
            _borrow: borrow,
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
