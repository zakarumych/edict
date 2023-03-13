use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutableQuery, IntoQuery, Query},
    relation::{OriginComponent, Relation},
};

/// Fetch for the [`FilterNotRelatesTo<R>`] query.
pub struct FetchFilterNotRelatesTo<'a, R: Relation> {
    kind: FetchKind<'a, R>,
}

enum FetchKind<'a, R: Relation> {
    /// Variant for entities without relation
    NotRelates,

    /// Variant for entities with relation
    Relates {
        target: EntityId,
        ptr: NonNull<OriginComponent<R>>,
        marker: PhantomData<&'a OriginComponent<R>>,
    },
}

use FetchKind::{NotRelates, Relates};

unsafe impl<'a, R> Fetch<'a> for FetchFilterNotRelatesTo<'a, R>
where
    R: Relation,
{
    type Item = ();

    #[inline]
    fn dangling() -> Self {
        FetchFilterNotRelatesTo { kind: NotRelates }
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
            NotRelates => false,
            Relates { ptr, target, .. } => {
                let origin_component = &*ptr.as_ptr().add(idx);
                origin_component
                    .origins()
                    .iter()
                    .all(|origin| origin.target != target)
            }
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, _: usize) -> () {}
}

/// Filters out relation origin with specified targets.
/// Yields entities that are not relation origins and origins of other targets.
pub struct FilterNotRelatesTo<R> {
    target: EntityId,
    phantom: PhantomData<R>,
}

impl_debug!(FilterNotRelatesTo<R> { target });

impl<R> FilterNotRelatesTo<R> {
    /// Returns relation filter bound to one specific target entity.
    pub const fn new(target: EntityId) -> Self {
        FilterNotRelatesTo {
            target,
            phantom: PhantomData,
        }
    }
}

impl<R> IntoQuery for FilterNotRelatesTo<R>
where
    R: Relation,
{
    type Query = Self;
}

unsafe impl<R> Query for FilterNotRelatesTo<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = FetchFilterNotRelatesTo<'a, R>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<OriginComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.has_component(TypeId::of::<OriginComponent<R>>())
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
    ) -> FetchFilterNotRelatesTo<'a, R> {
        match archetype.component(TypeId::of::<OriginComponent<R>>()) {
            None => FetchFilterNotRelatesTo { kind: NotRelates },
            Some(component) => {
                debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

                let data = component.data();

                FetchFilterNotRelatesTo {
                    kind: Relates {
                        target: self.target,
                        ptr: data.ptr.cast(),
                        marker: PhantomData,
                    },
                }
            }
        }
    }
}

unsafe impl<R> ImmutableQuery for FilterNotRelatesTo<R> where R: Relation {}

/// Returns a filter to filter out origins of relation with specified target.
pub fn not_relates_to<R: Relation>(target: EntityId) -> FilterNotRelatesTo<R> {
    FilterNotRelatesTo {
        target,
        phantom: PhantomData,
    }
}
