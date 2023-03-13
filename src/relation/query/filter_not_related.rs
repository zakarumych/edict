use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    query::{Access, ImmutablePhantomQuery, IntoQuery, PhantomQuery, UnitFetch},
    relation::{OriginComponent, Relation},
};

phantom_newtype! {
    /// Filters out targets of relation.
    pub struct FilterNotRelated<R>
}

impl<R> IntoQuery for FilterNotRelated<R>
where
    R: Relation,
{
    type Query = PhantomData<fn() -> FilterNotRelated<R>>;
}

unsafe impl<R> PhantomQuery for FilterNotRelated<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

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

unsafe impl<R> ImmutablePhantomQuery for FilterNotRelated<R> where R: Relation {}

/// Returns a filter to filter out targets of relation.
pub fn not_related<R: Relation>() -> PhantomData<FilterNotRelated<R>> {
    PhantomData
}
