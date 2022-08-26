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
/// [`PhantomData<Q>`] implements [`Query`] trait if `Q` implements [`Query`] trait.
pub unsafe trait PhantomQuery:
    for<'a> PhantomQueryFetch<'a> + IntoQuery<Query = PhantomData<Self>>
{
    /// Returns what kind of access the query performs on the component type.
    ///
    /// # Safety
    ///
    /// Soundness relies on the correctness of this method.
    fn access(ty: TypeId) -> Option<Access>;

    /// Returns access that requires strongest guarantees among all accesses the query performs.
    ///
    /// # Safety
    ///
    /// Soundness relies on the correctness of this method.
    fn access_any() -> Option<Access>;

    /// Returns `true` if query execution conflicts with another query.
    /// This method can be used by complex queries to implement `is_valid`.
    /// Another use case is within multithreaded scheduler to run non-conflicting queries in parallel.
    ///
    /// # Safety
    ///
    /// Soundness relies on the correctness of this method.
    fn conflicts<Q>(other: &Q) -> bool
    where
        Q: Query;

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

impl<Q> IntoQuery for PhantomData<Q>
where
    Q: PhantomQuery,
{
    type Query = PhantomData<Q>;
}

impl<'a, Q> QueryFetch<'a> for PhantomData<Q>
where
    Q: PhantomQuery,
{
    type Item = <Q as PhantomQueryFetch<'a>>::Item;
    type Fetch = <Q as PhantomQueryFetch<'a>>::Fetch;
}

unsafe impl<Q> Query for PhantomData<Q>
where
    Q: PhantomQuery,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Q as PhantomQuery>::access(ty)
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        <Q as PhantomQuery>::access_any()
    }

    #[inline]
    fn conflicts<U>(&self, other: &U) -> bool
    where
        U: Query,
    {
        <Q as PhantomQuery>::conflicts(other)
    }

    #[inline]
    fn is_valid(&self) -> bool {
        true
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

unsafe impl<Q> ImmutableQuery for PhantomData<Q> where Q: ImmutablePhantomQuery {}

impl<'a, T> QueryArgGet<'a> for PhantomData<T>
where
    T: PhantomQuery + 'static,
{
    type Arg = T;
    type Query = PhantomData<T>;

    fn get(&mut self, _world: &World) -> Self::Query {
        PhantomData
    }
}

impl<T> QueryArgCache for PhantomData<T>
where
    T: PhantomQuery + 'static,
{
    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        <T as PhantomQuery>::skip_archetype(archetype)
    }

    fn access_component(&self, id: TypeId) -> Option<Access> {
        <T as PhantomQuery>::access(id)
    }
}

impl<T> QueryArg for T
where
    T: PhantomQuery + 'static,
{
    type Cache = PhantomData<T>;
}
