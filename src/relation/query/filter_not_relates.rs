use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, ImmutablePhantomQuery, IntoQuery, PhantomQuery, PhantomQueryFetch, UnitFetch},
    relation::{OriginComponent, Relation},
};

phantom_newtype! {
    /// Filters out origins of relation.
    pub struct FilterNotRelates<R>
}

impl<R> IntoQuery for FilterNotRelates<R>
where
    R: Relation,
{
    type Query = PhantomData<fn() -> FilterNotRelates<R>>;
}

impl<R> PhantomQueryFetch<'_> for FilterNotRelates<R>
where
    R: Relation,
{
    type Item = ();
    type Fetch = UnitFetch;
}

unsafe impl<R> PhantomQuery for FilterNotRelates<R>
where
    R: Relation,
{
    #[inline]
    fn access(_: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<OriginComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(_: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<R> ImmutablePhantomQuery for FilterNotRelates<R> where R: Relation {}

/// Returns a filter to filter out origins of relation.
pub fn not_relates<R: Relation>() -> PhantomData<FilterNotRelates<R>> {
    PhantomData
}
