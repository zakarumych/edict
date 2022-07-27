//! Queries and iterators.
//!
//! To efficiently iterate over entities with specific set of components,
//! or only over those where specific component is modified, or missing,
//! [`Query`] is the solution.
//!
//! [`Query`] trait has a lot of implementations and is composable using tuples.

use core::any::TypeId;

use crate::{
    archetype::{chunk_idx, first_of_chunk, Archetype, CHUNK_LEN_USIZE},
    component::Component,
    entity::EntityId,
};

pub use self::{
    alt::{Alt, FetchAlt},
    fetch::{Fetch, VerifyFetch},
    filter::{Filter, FilteredFetch, FilteredQuery, With, Without},
    iter::QueryIter,
    modified::{Modified, ModifiedFetchAlt, ModifiedFetchRead, ModifiedFetchWrite},
    phantom::{ImmutablePhantomQuery, PhantomQuery, PhantomQueryItem},
    read::{read, FetchRead},
    write::{write, FetchWrite},
};

#[cfg(feature = "relation")]
pub use self::relation::{related, Related, RelatedFetchRead, RelatedReadIter};

mod alt;
mod fetch;
mod filter;
mod iter;
mod modified;
mod option;
mod phantom;
mod read;
#[cfg(feature = "relation")]
mod relation;
#[cfg(feature = "rc")]
mod skip;
mod tuple;
mod write;

/// Specifies kind of access query performs for particular component.
#[derive(Clone, Copy, Debug)]
pub enum Access {
    /// Read-only access to component versions.
    /// Can be aliased with [`Access::Write`] in single query.
    Track,

    /// Shared access to component. Can be aliased with other [`Access::Read`] and [`Access::Track`] accesses.
    Read,

    /// Cannot be aliased with other [`Access::Read`] access and [`Access::Track`] access in other queries.
    Write,
}

/// Trait to query components from entities in the world.
/// Queries implement efficient iteration over entities while yielding
/// references to the components and optionally `EntityId` to address same components later.
pub unsafe trait Query {
    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch: for<'a> Fetch<'a>;

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

    /// Returns what kind of access the query performs on the component type.
    ///
    /// # Safety
    ///
    /// Soundness relies on the correctness of this method.
    fn access(&self, ty: TypeId) -> Option<Access>;

    /// Checks if archetype must be skipped.
    fn skip_archetype(&self, archetype: &Archetype) -> bool;

    /// Fetches data from one archetype.
    ///
    /// # Safety
    ///
    /// Must not be called if `skip_archetype` returned `true`.
    unsafe fn fetch(&mut self, archetype: &Archetype, epoch: u64) -> Self::Fetch;
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
        type Item = Self;

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
        unsafe fn get_item(&mut self, _: usize) -> Self {
            match *self {}
        }
    }

    unsafe impl Query for QuasiQueryThatReadsEverything {
        type Fetch = QuasiFetchThatReadsEverything;

        #[inline]
        fn is_valid(&self) -> bool {
            true
        }

        fn conflicts<Q>(&self, _: &Q) -> bool
        where
            Q: Query,
        {
            unimplemented!()
        }

        fn access(&self, _: TypeId) -> Option<Access> {
            Some(Access::Read)
        }

        fn skip_archetype(&self, _: &Archetype) -> bool {
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
pub type QueryItem<'a, Q> = <<Q as Query>::Fetch as Fetch<'a>>::Item;

/// Mutable query builder.
#[allow(missing_debug_implementations)]
pub struct QueryMut<'a, Q, F> {
    archetypes: &'a [Archetype],
    epoch: &'a mut u64,
    query: Q,
    filter: F,
}

impl<'a, Q> QueryMut<'a, Q, ()> {
    pub(crate) fn new(archetypes: &'a [Archetype], epoch: &'a mut u64, query: Q) -> Self {
        QueryMut {
            archetypes,
            epoch,
            query,
            filter: (),
        }
    }
}

impl<'a, Q, F> QueryMut<'a, Q, F>
where
    Q: Query,
    F: Filter,
{
    /// Creates new layer of tuples of mutable query.
    pub fn layer(self) -> QueryMut<'a, (Q,), F> {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: (self.query,),
            filter: self.filter,
        }
    }

    /// Adds filter that skips entities that don't have specified component.
    pub fn with<T>(self) -> QueryMut<'a, Q, (With<T>, F)>
    where
        T: Component,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query,
            filter: (With::new(), self.filter),
        }
    }

    /// Adds filter that skips entities that have specified component.
    pub fn without<T>(self) -> QueryMut<'a, Q, (Without<T>, F)>
    where
        T: Component,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query,
            filter: (Without::new(), self.filter),
        }
    }

    /// Returns iterator over immutable query results.
    /// This method is only available with non-tracking queries.
    pub fn iter<'b>(&'b self) -> QueryIter<'b, FilteredQuery<F, Q>>
    where
        Q: ImmutableQuery + Clone,
        F: Clone,
    {
        debug_assert_immutable_query(&self.filter);
        debug_assert_immutable_query(&self.query);

        QueryIter::new(
            FilteredQuery {
                filter: self.filter.clone(),
                query: self.query.clone(),
            },
            *self.epoch,
            self.archetypes,
        )
    }

    /// Returns iterator over query results.
    /// This method is only available with non-tracking queries.
    pub fn iter_mut<'b>(&'b mut self) -> QueryIter<'b, FilteredQuery<F, Q>>
    where
        Q: Clone,
        F: Clone,
    {
        debug_assert_immutable_query(&self.filter);

        *self.epoch += 1;
        QueryIter::new(
            FilteredQuery {
                filter: self.filter.clone(),
                query: self.query.clone(),
            },
            *self.epoch,
            self.archetypes,
        )
    }

    /// Returns iterator over query results.
    /// This method is only available with non-tracking queries.
    pub fn into_iter(self) -> QueryIter<'a, FilteredQuery<F, Q>> {
        debug_assert_immutable_query(&self.filter);

        *self.epoch += 1;
        QueryIter::new(
            FilteredQuery {
                filter: self.filter,
                query: self.query,
            },
            *self.epoch,
            self.archetypes,
        )
    }

    /// Iterates through world using specified query.
    ///
    /// This method can be used for queries that mutate components.
    /// This method only works queries that does not track for component changes.
    #[inline]
    pub fn for_each_mut<Fun>(self, f: Fun)
    where
        Q: Query,
        Fun: FnMut(QueryItem<'_, Q>),
    {
        assert!(self.filter.is_valid(), "Invalid query specified");
        assert!(self.query.is_valid(), "Invalid query specified");

        debug_assert_immutable_query(&self.filter);

        *self.epoch += 1;

        for_each_impl(self.filter, self.query, self.archetypes, *self.epoch, f)
    }

    /// Iterates through world using specified query.
    ///
    /// This method can be used for queries that mutate components.
    /// This method only works queries that does not track for component changes.
    #[inline]
    pub fn for_each<Fun>(self, f: Fun)
    where
        Q: ImmutableQuery,
        Fun: FnMut(QueryItem<'_, Q>),
    {
        assert!(self.filter.is_valid(), "Invalid query specified");
        assert!(self.query.is_valid(), "Invalid query specified");

        debug_assert_immutable_query(&self.filter);
        debug_assert_immutable_query(&self.query);

        for_each_impl(self.filter, self.query, self.archetypes, *self.epoch, f)
    }
}

impl<'a, Q, F> IntoIterator for QueryMut<'a, Q, F>
where
    Q: Query,
    F: Filter,
{
    type Item = (EntityId, QueryItem<'a, Q>);
    type IntoIter = QueryIter<'a, FilteredQuery<F, Q>>;

    fn into_iter(self) -> QueryIter<'a, FilteredQuery<F, Q>> {
        self.into_iter()
    }
}
/// Immutable query builder.
#[derive(Clone, Copy)]
#[allow(missing_debug_implementations)]
pub struct QueryRef<'a, Q, F> {
    archetypes: &'a [Archetype],
    epoch: u64,
    query: Q,
    filter: F,
}

impl<'a, Q> QueryRef<'a, Q, ()>
where
    Q: ImmutableQuery,
{
    pub(crate) fn new(archetypes: &'a [Archetype], epoch: u64, query: Q) -> Self {
        QueryRef {
            archetypes,
            epoch,
            query,
            filter: (),
        }
    }
}

impl<'a, Q, F> QueryRef<'a, Q, F>
where
    Q: ImmutableQuery,
    F: Filter,
{
    /// Creates new layer of tuples of immutable query.
    pub fn layer(self) -> QueryRef<'a, (Q,), F> {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: (self.query,),
            filter: self.filter,
        }
    }

    /// Returns iterator over immutable query results.
    /// This method is only available with non-tracking queries.
    pub fn iter<'b>(&'b self) -> QueryIter<'b, FilteredQuery<F, Q>>
    where
        Q: Clone,
        F: Clone,
    {
        debug_assert_immutable_query(&self.query);
        debug_assert_immutable_query(&self.filter);

        QueryIter::new(
            FilteredQuery {
                filter: self.filter.clone(),
                query: self.query.clone(),
            },
            self.epoch,
            self.archetypes,
        )
    }

    /// Returns iterator over immutable query results.
    /// This method is only available with non-tracking queries.
    pub fn into_iter(self) -> QueryIter<'a, FilteredQuery<F, Q>> {
        debug_assert_immutable_query(&self.query);
        debug_assert_immutable_query(&self.filter);

        QueryIter::new(
            FilteredQuery {
                filter: self.filter,
                query: self.query,
            },
            self.epoch,
            self.archetypes,
        )
    }

    /// Iterates through world using specified query.
    ///
    /// This method can be used for queries that mutate components.
    /// This method only works queries that does not track for component changes.
    #[inline]
    pub fn for_each<Fun>(self, f: Fun)
    where
        Q: ImmutableQuery,
        Fun: FnMut(QueryItem<'_, Q>),
    {
        assert!(self.filter.is_valid(), "Invalid query specified");
        assert!(self.query.is_valid(), "Invalid query specified");

        debug_assert_immutable_query(&self.filter);
        debug_assert_immutable_query(&self.query);

        for_each_impl(self.filter, self.query, self.archetypes, self.epoch, f)
    }
}

impl<'a, Q, F> IntoIterator for QueryRef<'a, Q, F>
where
    Q: ImmutableQuery,
    F: Filter,
{
    type Item = (EntityId, QueryItem<'a, Q>);
    type IntoIter = QueryIter<'a, FilteredQuery<F, Q>>;

    fn into_iter(self) -> QueryIter<'a, FilteredQuery<F, Q>> {
        self.into_iter()
    }
}

const fn merge_access(lhs: Option<Access>, rhs: Option<Access>) -> Option<Access> {
    match (lhs, rhs) {
        (None, rhs) => rhs,
        (lhs, None) => lhs,
        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
        _ => Some(Access::Write),
    }
}

fn for_each_impl<Q, F, Fun>(filter: F, query: Q, archetypes: &[Archetype], epoch: u64, mut f: Fun)
where
    Q: Query,
    F: Filter,
    Fun: FnMut(QueryItem<'_, Q>),
{
    let mut query = FilteredQuery {
        filter: filter,
        query: query,
    };

    for archetype in archetypes {
        if query.skip_archetype(archetype) {
            continue;
        }

        let mut fetch = unsafe { query.fetch(archetype, epoch) };

        let mut indices = 0..archetype.len();
        let mut visit_chunk = false;

        while let Some(idx) = indices.next() {
            if let Some(chunk_idx) = first_of_chunk(idx) {
                if unsafe { fetch.skip_chunk(chunk_idx) } {
                    indices.nth(CHUNK_LEN_USIZE - 1);
                    continue;
                }
                visit_chunk = true;
            }

            if !unsafe { fetch.skip_item(idx) } {
                if visit_chunk {
                    unsafe { fetch.visit_chunk(chunk_idx(idx)) }
                    visit_chunk = false;
                }
                let item = unsafe { fetch.get_item(idx) };
                f(item);
            }
        }
    }
}

macro_rules! for_tuple {
    () => {
        for_tuple!(for A B C D E F G H I J K L M N O P);
    };

    (for) => {
        for_tuple!(impl);
    };

    (for $head:ident $($tail:ident)*) => {
        for_tuple!(for $($tail)*);
        for_tuple!(impl $head $($tail)*);
    };

    (impl $($a:ident)*) => {
        impl<'a, $($a,)* Y> QueryMut<'a, ($($a,)*), Y> {
            /// Adds query to fetch modified components.
            pub fn modified<T>(self, epoch: u64) -> QueryMut<'a, ($($a,)* Modified<T>,), Y> {
                #![allow(non_snake_case)]

                let ($($a,)*) = self.query;

                QueryMut {
                    archetypes: self.archetypes,
                    epoch: self.epoch,
                    query: ($($a,)* Modified::<T>::new(epoch),),
                    filter: self.filter,
                }
            }
        }

        impl<'a, $($a,)* Y> QueryRef<'a, ($($a,)*), Y> {
            /// Adds query to fetch modified components.
            pub fn modified<T>(self, epoch: u64) -> QueryRef<'a, ($($a,)* Modified<T>,), Y> {
                #![allow(non_snake_case)]

                let ($($a,)*) = self.query;

                QueryRef {
                    archetypes: self.archetypes,
                    epoch: self.epoch,
                    query: ($($a,)* Modified::<T>::new(epoch),),
                    filter: self.filter,
                }
            }
        }
    };
}

for_tuple!();
