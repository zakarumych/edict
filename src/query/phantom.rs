use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::{fetch::Fetch, Access, ImmutableQuery, IntoQuery, Query, QueryFetch};

/// HRTB for `PhantomQuery` trait.
pub trait PhantomQueryFetch<'a> {
    /// Item type this query type yields.
    type Item: 'a;

    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch: Fetch<'a, Item = Self::Item>;
}

/// Phantom counterpart of [`Query`] trait.
/// This trait has all the same methods without `self` argument.
///
/// [`PhantomData<fn() -> Q>`] implements [`Query`] trait if `Q` implements [`Query`] trait.
pub trait PhantomQuery:
    for<'a> PhantomQueryFetch<'a> + IntoQuery<Query = PhantomData<fn() -> Self>>
{
    /// Returns what kind of access the query performs on the component type.
    fn access(ty: TypeId) -> Option<Access>;

    /// Checks if archetype must be skipped.
    fn skip_archetype(archetype: &Archetype) -> bool;

    /// Fetches data from one archetype.
    ///
    /// # Safety
    ///
    /// Must not be called if `skip_archetype` returned `true`.
    unsafe fn fetch<'a>(
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> <Self as PhantomQueryFetch<'a>>::Fetch;
}

impl<Q> IntoQuery for PhantomData<fn() -> Q>
where
    Q: PhantomQuery,
{
    type Query = PhantomData<fn() -> Q>;
}

impl<'a, Q> QueryFetch<'a> for PhantomData<fn() -> Q>
where
    Q: PhantomQuery,
{
    type Item = <Q as PhantomQueryFetch<'a>>::Item;
    type Fetch = <Q as PhantomQueryFetch<'a>>::Fetch;
}

impl<Q> Query for PhantomData<fn() -> Q>
where
    Q: PhantomQuery,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Q as PhantomQuery>::access(ty)
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        <Q as PhantomQuery>::skip_archetype(archetype)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> <Self as QueryFetch<'a>>::Fetch {
        <Q as PhantomQuery>::fetch(archetype, epoch)
    }
}

/// Phantom counterpart of [`ImmutableQuery`] type alias.
pub unsafe trait ImmutablePhantomQuery: PhantomQuery {}

unsafe impl<Q> ImmutableQuery for PhantomData<fn() -> Q> where Q: ImmutablePhantomQuery {}

impl<'a, T> QueryArgGet<'a> for PhantomData<fn() -> T>
where
    T: PhantomQuery + 'static,
{
    type Arg = T;
    type Query = PhantomData<fn() -> T>;

    #[inline]
    fn get(&mut self, _world: &World) -> Self::Query {
        PhantomData
    }
}

impl<T> QueryArgCache for PhantomData<fn() -> T>
where
    T: PhantomQuery + 'static,
{
    #[inline]
    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        <T as PhantomQuery>::skip_archetype(archetype)
    }

    #[inline]
    fn access_component(&self, id: TypeId) -> Option<Access> {
        <T as PhantomQuery>::access(id)
    }
}

impl<T> QueryArg for T
where
    T: PhantomQuery + 'static,
{
    type Cache = PhantomData<fn() -> T>;
}
