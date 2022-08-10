use core::{any::TypeId, marker::PhantomData};

use crate::archetype::Archetype;

use super::{fetch::Fetch, Access, ImmutableQuery, Query, QueryFetch};

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
pub unsafe trait PhantomQuery: for<'a> PhantomQueryFetch<'a> {
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

    /// Function to validate that query does not cause mutable reference aliasing.
    /// e.g. `(&mut T, &mut T)` is not valid query, but `(&T, &T)` is.
    ///
    /// Attempt to run invalid query will result in panic.
    /// It is always user's responsibility to ensure that query is valid.
    ///
    /// Typical query validity does not depend on the runtime values.
    /// So it should be possible to ensure that query is valid by looking at its type.
    ///
    /// # Safety
    ///
    /// Soundness relies on the correctness of this method.
    ///
    /// This method is called before running the query.
    /// It must return `true` only if it is sound to call `fetch` for any archetype
    /// that won't be skipped and get items from it.
    fn is_valid() -> bool;

    /// Checks if archetype must be skipped.
    fn skip_archetype(archetype: &Archetype) -> bool;

    /// Fetches data from one archetype.
    ///
    /// # Safety
    ///
    /// Must not be called if `skip_archetype` returned `true`.
    unsafe fn fetch<'a>(
        archetype: &'a Archetype,
        epoch: u64,
    ) -> <Self as PhantomQueryFetch<'a>>::Fetch;
}

/// Phantom counterpart of [`QueryItem`] type alias.
///
/// [`QueryItem`]: trait.Query.html#associatedtype.QueryItem
pub type PhantomQueryItem<'a, Q> = <Q as PhantomQueryFetch<'a>>::Item;

impl<'a, Q> QueryFetch<'a> for PhantomData<Q>
where
    Q: PhantomQuery,
{
    type Item = PhantomQueryItem<'a, Q>;
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
        <Q as PhantomQuery>::is_valid()
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        <Q as PhantomQuery>::skip_archetype(archetype)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: u64,
    ) -> <Self as QueryFetch<'a>>::Fetch {
        <Q as PhantomQuery>::fetch(archetype, epoch)
    }
}

/// Phantom counterpart of [`ImmutableQuery`] type alias.
pub unsafe trait ImmutablePhantomQuery: PhantomQuery {}

unsafe impl<Q> ImmutableQuery for PhantomData<Q> where Q: ImmutablePhantomQuery {}
