use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::AtomicBorrow;

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query, QueryFetch},
    relation::{Relation, TargetComponent},
};

/// Fetch for the [`FilterNotRelatedBy<R>`] query.
pub struct FetchFilterNotRelatedBy<'a, R: Relation> {
    kind: FetchKind<'a, R>,
}

enum FetchKind<'a, R: Relation> {
    NotRelated,
    Related {
        origin: EntityId,
        ptr: NonNull<TargetComponent<R>>,
        _borrow: AtomicBorrow<'a>,
        marker: PhantomData<&'a TargetComponent<R>>,
    },
}

use FetchKind::{NotRelated, Related};

unsafe impl<'a, R> Fetch<'a> for FetchFilterNotRelatedBy<'a, R>
where
    R: Relation,
{
    type Item = ();

    #[inline]
    fn dangling() -> Self {
        FetchFilterNotRelatedBy { kind: NotRelated }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        match self.kind {
            NotRelated => false,
            Related { ptr, origin, .. } => {
                let target_component = &*ptr.as_ptr().add(idx);
                target_component.origins.contains(&origin)
            }
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, _: usize) -> () {}
}

/// Filters out relation targets with specified origin.
/// Yields entities that are not relation targets and targets of other origins.
pub struct FilterNotRelatedBy<R> {
    origin: EntityId,
    phantom: PhantomData<R>,
}

phantom_debug!(FilterNotRelatedBy<R> { origin });

impl<R> FilterNotRelatedBy<R> {
    /// Returns relation filter bound to one specific origin.
    pub const fn new(origin: EntityId) -> Self {
        FilterNotRelatedBy {
            origin,
            phantom: PhantomData,
        }
    }
}

impl<'a, R> QueryFetch<'a> for FilterNotRelatedBy<R>
where
    R: Relation,
{
    type Item = ();
    type Fetch = FetchFilterNotRelatedBy<'a, R>;
}

impl<R> IntoQuery for FilterNotRelatedBy<R>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> Query for FilterNotRelatedBy<R>
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
    ) -> FetchFilterNotRelatedBy<'a, R> {
        match archetype.id_index(TypeId::of::<TargetComponent<R>>()) {
            None => FetchFilterNotRelatedBy { kind: NotRelated },
            Some(idx) => {
                let component = archetype.component(idx);
                debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

                let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

                FetchFilterNotRelatedBy {
                    kind: Related {
                        origin: self.origin,
                        ptr: data.ptr.cast(),
                        _borrow: borrow,
                        marker: PhantomData,
                    },
                }
            }
        }
    }
}

unsafe impl<R> ImmutableQuery for FilterNotRelatedBy<R> where R: Relation {}

/// Returns a filter to filter out targets of relation with specified origin.
pub fn not_related_by<R: Relation>(origin: EntityId) -> FilterNotRelatedBy<R> {
    FilterNotRelatedBy {
        origin,
        phantom: PhantomData,
    }
}
