//! Queries and iterators.
//!
//! To efficiently iterate over entities with specific set of components,
//! or only over those where specific component is modified, or missing,
//! [`Query`] is the solution.
//!
//! [`Query`] trait has a lot of implementations and is composable using tuples.

use core::any::TypeId;

use crate::archetype::Archetype;

pub use self::{
    alt::{Alt, FetchAlt},
    borrow::{
        FetchBorrowAllRead, FetchBorrowAnyRead, FetchBorrowAnyWrite, FetchBorrowOneRead,
        FetchBorrowOneWrite, QueryBorrowAll, QueryBorrowAny, QueryBorrowOne,
    },
    fetch::{Fetch, VerifyFetch},
    filter::{Filter, FilteredFetch, FilteredQuery, With, Without},
    iter::QueryIter,
    modified::{Modified, ModifiedFetchAlt, ModifiedFetchRead, ModifiedFetchWrite},
    phantom::{ImmutablePhantomQuery, PhantomQuery, PhantomQueryFetch, PhantomQueryItem},
    read::{read, FetchRead},
    write::{write, FetchWrite},
};

mod alt;
mod borrow;
mod fetch;
mod filter;
mod iter;
mod modified;
mod option;
mod phantom;
mod read;
mod tuple;
mod write;

/// Specifies kind of access query performs for particular component.
#[derive(Clone, Copy, Debug)]
pub enum Access {
    /// Shared access to component. Can be aliased with other [`Access::Read`] accesses.
    Read,

    /// Cannot be aliased with any other access.
    Write,
}

/// HRKT for `Query` trait.
pub trait QueryFetch<'a> {
    /// Item type this query type yields.
    type Item: 'a;

    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch: Fetch<'a, Item = Self::Item>;
}

/// Trait to query components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// references to the components and optionally `EntityId` to address same components later.
pub unsafe trait Query: for<'a> QueryFetch<'a> {
    /// Returns what kind of access the query performs on the component type.
    ///
    /// # Safety
    ///
    /// Soundness relies on the correctness of this method.
    fn access(&self, ty: TypeId) -> Option<Access>;

    /// Returns access that requires strongest guarantees among all accesses the query performs.
    ///
    /// # Safety
    ///
    /// Soundness relies on the correctness of this method.
    fn access_any(&self) -> Option<Access>;

    /// Returns `true` if query execution conflicts with another query.
    /// This method can be used by complex queries to implement `is_valid`.
    /// Another use case is within multithreaded scheduler to run non-conflicting queries in parallel.
    ///
    /// # Safety
    ///
    /// Soundness relies on the correctness of this method.
    fn conflicts<Q>(&self, other: &Q) -> bool
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
    fn is_valid(&self) -> bool;

    /// Checks if archetype must be skipped.
    /// Without taking into account modifiable state of the archetype.
    fn skip_archetype_unconditionally(&self, archetype: &Archetype) -> bool;

    /// Checks if archetype must be skipped.
    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        self.skip_archetype_unconditionally(archetype)
    }

    /// Fetches data from one archetype.
    ///
    /// # Safety
    ///
    /// Must not be called if `skip_archetype` returned `true`.
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: u64,
    ) -> <Self as QueryFetch<'a>>::Fetch;
}

/// Query that does not mutate any components.
///
/// # Safety
///
/// `Query::mutate` must return `false`.
/// `Query` must not borrow components mutably.
/// `Query` must not modify entities versions.
pub unsafe trait ImmutableQuery: Query {}

/// Asserts that query is indeed immutable
/// by checking that it doesn't conflict with query that reads everything.
pub(crate) fn assert_immutable_query(query: &impl ImmutableQuery) {
    struct QuasiQueryThatReadsEverything;
    enum QuasiFetchThatReadsEverything {}

    unsafe impl<'a> Fetch<'a> for QuasiFetchThatReadsEverything {
        type Item = ();

        #[inline]
        fn dangling() -> Self {
            unimplemented!()
        }

        #[inline]
        unsafe fn skip_chunk(&mut self, _: usize) -> bool {
            match *self {}
        }

        #[inline]
        unsafe fn visit_chunk(&mut self, _: usize) {
            match *self {}
        }

        #[inline]
        unsafe fn skip_item(&mut self, _: usize) -> bool {
            match *self {}
        }

        #[inline]
        unsafe fn get_item(&mut self, _: usize) {
            match *self {}
        }
    }

    impl QueryFetch<'_> for QuasiQueryThatReadsEverything {
        type Item = ();
        type Fetch = QuasiFetchThatReadsEverything;
    }

    unsafe impl Query for QuasiQueryThatReadsEverything {
        fn access(&self, _: TypeId) -> Option<Access> {
            Some(Access::Read)
        }

        fn access_any(&self) -> Option<Access> {
            Some(Access::Read)
        }

        fn conflicts<Q>(&self, _: &Q) -> bool
        where
            Q: Query,
        {
            unimplemented!()
        }

        #[inline]
        fn is_valid(&self) -> bool {
            true
        }

        fn skip_archetype_unconditionally(&self, _: &Archetype) -> bool {
            unimplemented!()
        }

        unsafe fn fetch(&mut self, _: &Archetype, _: u64) -> QuasiFetchThatReadsEverything {
            unimplemented!()
        }
    }

    assert_eq!(
        query.conflicts(&QuasiQueryThatReadsEverything),
        false,
        "Immutable query must not conflict with query that reads everything"
    );
}

#[inline]
pub(crate) fn debug_assert_immutable_query(query: &impl ImmutableQuery) {
    #[cfg(debug_assertions)]
    assert_immutable_query(query);
}

/// Type alias for items returned by the query type.
pub type QueryItem<'a, Q> = <Q as QueryFetch<'a>>::Item;

/// Merge two optional access values.
pub const fn merge_access(lhs: Option<Access>, rhs: Option<Access>) -> Option<Access> {
    match (lhs, rhs) {
        (None, rhs) => rhs,
        (lhs, None) => lhs,
        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
        _ => Some(Access::Write),
    }
}
