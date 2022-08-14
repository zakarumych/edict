use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::{chunk_idx, first_of_chunk, Archetype, CHUNK_LEN_USIZE},
    component::Component,
    entity::EntityId,
    query::{
        debug_assert_immutable_query, Fetch, Filter, FilteredQuery, Modified, PhantomQuery, Query,
        QueryBorrowAll, QueryBorrowAny, QueryBorrowOne, QueryItem, QueryIter, With, Without,
    },
    relation::{QueryRelated, QueryRelation, QueryRelationTo, Relation, WithRelationTo},
};

pub trait ExtendTuple<E>: Sized {
    type Output;

    fn extend_tuple(self, element: E) -> Self::Output;
}

macro_rules! for_tuple {
    () => {
        for_tuple!(for A B C D E F G H I J K L M N O);
        // for_tuple!(for A);
    };

    (for) => {
        for_tuple!(impl);
    };

    (for $head:ident $($tail:ident)*) => {
        for_tuple!(for $($tail)*);
        for_tuple!(impl $head $($tail)*);
    };

    (impl $($a:ident)*) => {
        impl<U $(, $a)*> ExtendTuple<U> for ($($a,)*)
        {
            type Output = ($($a,)* U,);

            #[inline]
            fn extend_tuple(self, other: U) -> Self::Output {
                #![allow(non_snake_case)]
                let ($($a,)*) = self;
                 ($($a,)* other,)
            }
        }
    };
}

for_tuple!();

/// Mutable query builder.
#[allow(missing_debug_implementations)]
pub struct QueryMut<'a, Q, F> {
    archetypes: &'a [Archetype],
    epoch: &'a mut u64,
    query: Q,
    filter: F,
}

impl<'a, Q, F> QueryMut<'a, Q, F>
where
    Q: Query,
    F: Filter,
{
    pub(crate) fn new(
        archetypes: &'a [Archetype],
        epoch: &'a mut u64,
        query: Q,
        filter: F,
    ) -> Self {
        QueryMut {
            archetypes,
            epoch,
            query,
            filter,
        }
    }

    /// Creates new layer of tuples of mutable query.
    pub fn layer(self) -> QueryMut<'a, (Q,), F> {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: (self.query,),
            filter: self.filter,
        }
    }

    /// Adds specified query.
    pub fn extend_query<T>(self, query: T) -> QueryMut<'a, <Q as ExtendTuple<T>>::Output, F>
    where
        T: Query,
        Q: ExtendTuple<T>,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(query),
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

    /// Adds query to fetch relation.
    pub fn with_relation_to<R>(self, target: EntityId) -> QueryMut<'a, Q, (WithRelationTo<R>, F)>
    where
        R: Relation,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query,
            filter: (WithRelationTo::new(target), self.filter),
        }
    }

    /// Adds query to fetch modified components.
    pub fn modified<T>(self, epoch: u64) -> QueryMut<'a, <Q as ExtendTuple<Modified<T>>>::Output, F>
    where
        Modified<T>: Query,
        Q: ExtendTuple<Modified<T>>,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(Modified::<T>::new(epoch)),
            filter: self.filter,
        }
    }

    /// Extends query to borrow from components.
    pub fn borrow_any<T>(
        self,
    ) -> QueryMut<'a, <Q as ExtendTuple<PhantomData<QueryBorrowAny<T>>>>::Output, F>
    where
        QueryBorrowAny<T>: PhantomQuery,
        Q: ExtendTuple<PhantomData<QueryBorrowAny<T>>>,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData::<QueryBorrowAny<T>>),
            filter: self.filter,
        }
    }

    /// Extends query to borrow from components.
    pub fn borrow_one<T>(
        self,
        id: TypeId,
    ) -> QueryMut<'a, <Q as ExtendTuple<QueryBorrowOne<T>>>::Output, F>
    where
        QueryBorrowOne<T>: Query,
        Q: ExtendTuple<QueryBorrowOne<T>>,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(QueryBorrowOne::<T>::new(id)),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    pub fn relation<R>(
        self,
    ) -> QueryMut<'a, <Q as ExtendTuple<PhantomData<QueryRelation<R>>>>::Output, F>
    where
        QueryRelation<R>: PhantomQuery,
        Q: ExtendTuple<PhantomData<QueryRelation<R>>>,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData::<QueryRelation<R>>),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    pub fn relation_to<R>(
        self,
        entity: EntityId,
    ) -> QueryMut<'a, <Q as ExtendTuple<QueryRelationTo<R>>>::Output, F>
    where
        QueryRelationTo<R>: Query,
        Q: ExtendTuple<QueryRelationTo<R>>,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(QueryRelationTo::new(entity)),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    pub fn related<R>(
        self,
    ) -> QueryMut<'a, <Q as ExtendTuple<PhantomData<QueryRelated<R>>>>::Output, F>
    where
        QueryRelationTo<R>: Query,
        Q: ExtendTuple<PhantomData<QueryRelated<R>>>,
    {
        QueryMut {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData::<QueryRelated<R>>),
            filter: self.filter,
        }
    }

    /// Returns iterator over immutable query results.
    /// This method is only available with non-tracking queries.
    pub fn iter<'b>(&'b self) -> QueryIter<'b, FilteredQuery<F, Q>>
    where
        Q: Query + Clone,
        F: Clone,
    {
        debug_assert_immutable_query(&self.filter);

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
        Q: Query,
        Fun: FnMut(QueryItem<'_, Q>),
    {
        assert!(self.filter.is_valid(), "Invalid query specified");
        assert!(self.query.is_valid(), "Invalid query specified");

        debug_assert_immutable_query(&self.filter);

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
/// Query builder.
#[derive(Clone, Copy)]
#[allow(missing_debug_implementations)]
pub struct QueryRef<'a, Q, F> {
    archetypes: &'a [Archetype],
    epoch: u64,
    query: Q,
    filter: F,
}

impl<'a, Q, F> QueryRef<'a, Q, F>
where
    Q: Query,
    F: Filter,
{
    pub(crate) fn new(archetypes: &'a [Archetype], epoch: u64, query: Q, filter: F) -> Self {
        QueryRef {
            archetypes,
            epoch,
            query,
            filter,
        }
    }

    /// Creates new layer of tuples of immutable query.
    pub fn layer(self) -> QueryRef<'a, (Q,), F> {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: (self.query,),
            filter: self.filter,
        }
    }

    /// Adds specified query.
    pub fn extend_query<T>(self, query: T) -> QueryRef<'a, <Q as ExtendTuple<T>>::Output, F>
    where
        T: Query,
        Q: ExtendTuple<T>,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(query),
            filter: self.filter,
        }
    }

    /// Adds filter that skips entities that don't have specified component.
    pub fn with<T>(self) -> QueryRef<'a, Q, (With<T>, F)>
    where
        T: Component,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query,
            filter: (With::new(), self.filter),
        }
    }

    /// Adds filter that skips entities that have specified component.
    pub fn without<T>(self) -> QueryRef<'a, Q, (Without<T>, F)>
    where
        T: Component,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query,
            filter: (Without::new(), self.filter),
        }
    }

    /// Extends query to borrow from any viable component.
    pub fn borrow_any<T>(
        self,
    ) -> QueryRef<'a, <Q as ExtendTuple<PhantomData<QueryBorrowAny<T>>>>::Output, F>
    where
        QueryBorrowAny<T>: PhantomQuery,
        Q: ExtendTuple<PhantomData<QueryBorrowAny<T>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData::<QueryBorrowAny<T>>),
            filter: self.filter,
        }
    }

    /// Extends query to borrow from all viable components.
    pub fn borrow_all<T>(
        self,
    ) -> QueryRef<'a, <Q as ExtendTuple<PhantomData<QueryBorrowAll<T>>>>::Output, F>
    where
        QueryBorrowAll<T>: PhantomQuery,
        Q: ExtendTuple<PhantomData<QueryBorrowAll<T>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData::<QueryBorrowAll<T>>),
            filter: self.filter,
        }
    }

    /// Extends query to borrow from specific component.
    pub fn borrow_one<T>(
        self,
        id: TypeId,
    ) -> QueryRef<'a, <Q as ExtendTuple<QueryBorrowOne<T>>>::Output, F>
    where
        QueryBorrowOne<T>: Query,
        Q: ExtendTuple<QueryBorrowOne<T>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(QueryBorrowOne::<T>::new(id)),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    pub fn with_relation_to<R>(self, target: EntityId) -> QueryRef<'a, Q, (WithRelationTo<R>, F)>
    where
        R: Relation,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query,
            filter: (WithRelationTo::new(target), self.filter),
        }
    }

    /// Adds query to fetch modified components.
    pub fn modified<T>(self, epoch: u64) -> QueryRef<'a, <Q as ExtendTuple<Modified<T>>>::Output, F>
    where
        Modified<T>: Query,
        Q: ExtendTuple<Modified<T>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(Modified::<T>::new(epoch)),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    pub fn relation<R>(
        self,
    ) -> QueryRef<'a, <Q as ExtendTuple<PhantomData<QueryRelation<R>>>>::Output, F>
    where
        QueryRelation<R>: PhantomQuery,
        Q: ExtendTuple<PhantomData<QueryRelation<R>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData::<QueryRelation<R>>),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    pub fn relation_to<R>(
        self,
        entity: EntityId,
    ) -> QueryRef<'a, <Q as ExtendTuple<QueryRelationTo<R>>>::Output, F>
    where
        QueryRelationTo<R>: Query,
        Q: ExtendTuple<QueryRelationTo<R>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(QueryRelationTo::new(entity)),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    pub fn related<R>(
        self,
    ) -> QueryRef<'a, <Q as ExtendTuple<PhantomData<QueryRelated<R>>>>::Output, F>
    where
        QueryRelationTo<R>: Query,
        Q: ExtendTuple<PhantomData<QueryRelated<R>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData::<QueryRelated<R>>),
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
        Q: Query,
        Fun: FnMut(QueryItem<'_, Q>),
    {
        assert!(self.filter.is_valid(), "Invalid query specified");
        assert!(self.query.is_valid(), "Invalid query specified");

        debug_assert_immutable_query(&self.filter);

        for_each_impl(self.filter, self.query, self.archetypes, self.epoch, f)
    }
}

impl<'a, Q, F> IntoIterator for QueryRef<'a, Q, F>
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
