use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query},
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

impl_debug!(FilterNotRelatedBy<R> { origin });

impl<R> FilterNotRelatedBy<R> {
    /// Returns relation filter bound to one specific origin.
    pub const fn new(origin: EntityId) -> Self {
        FilterNotRelatedBy {
            origin,
            phantom: PhantomData,
        }
    }
}

impl<R> IntoQuery for FilterNotRelatedBy<R>
where
    R: Relation,
{
    type Query = Self;
}

unsafe impl<R> Query for FilterNotRelatedBy<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = FetchFilterNotRelatedBy<'a, R>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<TargetComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn skip_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        if archetype.has_component(TypeId::of::<TargetComponent<R>>()) {
            f(TypeId::of::<TargetComponent<R>>(), Access::Read)
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchFilterNotRelatedBy<'a, R> {
        match archetype.component(TypeId::of::<TargetComponent<R>>()) {
            None => FetchFilterNotRelatedBy { kind: NotRelated },
            Some(component) => {
                debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

                let data = component.data();

                FetchFilterNotRelatedBy {
                    kind: Related {
                        origin: self.origin,
                        ptr: data.ptr.cast(),
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
