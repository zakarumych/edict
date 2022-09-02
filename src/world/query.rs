use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::{chunk_idx, first_of_chunk, Archetype, CHUNK_LEN_USIZE},
    component::Component,
    entity::{Entities, EntityId},
    query::{
        Fetch, Filter, FilteredQuery, IntoFilter, IntoQuery, Modified, PhantomQuery, Query,
        QueryBorrowAll, QueryBorrowAny, QueryBorrowOne, QueryItem, QueryIter, With, Without,
    },
    relation::{QueryRelated, QueryRelation, QueryRelationTo, Relation, WithRelationTo},
    world::QueryOneError,
};

use super::{EpochCounter, EpochId, World};

pub trait ExtendTuple<E>: Sized {
    type Output;

    fn extend_tuple(self, element: E) -> Self::Output;
}

pub type TuplePlus<T, E> = <T as ExtendTuple<E>>::Output;

macro_rules! for_tuple {
    () => {
        for_tuple!(for A B C D E F G H I J K L M N O P Q R S T U V W X Y Z);
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
        impl<Add $(, $a)*> ExtendTuple<Add> for ($($a,)*)
        {
            type Output = ($($a,)* Add,);

            #[inline]
            fn extend_tuple(self, other: Add) -> Self::Output {
                #![allow(non_snake_case)]
                let ($($a,)*) = self;
                 ($($a,)* other,)
            }
        }
    };
}

for_tuple!();

/// Mutable query builder.
pub struct QueryRef<'a, Q: IntoQuery, F: IntoQuery = ()> {
    archetypes: &'a [Archetype],
    entities: &'a Entities,
    epoch: &'a EpochCounter,
    query: Q::Query,
    filter: F::Query,
}

impl<'a, Q, F> QueryRef<'a, Q, F>
where
    Q: IntoQuery,
    F: IntoQuery,
{
    /// Constructs query from query part, filter part and world.
    #[inline]
    pub fn new(world: &'a World, query: Q::Query, filter: F::Query) -> Self {
        QueryRef {
            archetypes: world.archetypes(),
            entities: &world.entities,
            epoch: world.epoch_counter(),
            query,
            filter,
        }
    }

    /// Creates new layer of tuples of mutable query.
    #[inline]
    pub fn layer(self) -> QueryRef<'a, (Q,), F> {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: (self.query,),
            filter: self.filter,
        }
    }

    /// Adds specified query.
    #[inline]
    pub fn extend_query<T>(self, query: T) -> QueryRef<'a, TuplePlus<Q, T>, F>
    where
        T: Query,
        Q: ExtendTuple<T>,
        Q::Query: ExtendTuple<T>,
        TuplePlus<Q, T>: IntoQuery<Query = TuplePlus<Q::Query, T>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query.extend_tuple(query),
            filter: self.filter,
        }
    }

    /// Adds filter that skips entities that don't have specified component.
    #[inline]
    pub fn with<T>(self) -> QueryRef<'a, Q, (With<T>, F)>
    where
        T: Component,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query,
            filter: (PhantomData, self.filter),
        }
    }

    /// Adds filter that skips entities that have specified component.
    #[inline]
    pub fn without<T>(self) -> QueryRef<'a, Q, (Without<T>, F)>
    where
        T: Component,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query,
            filter: (PhantomData, self.filter),
        }
    }

    /// Adds query to fetch relation.
    #[inline]
    pub fn with_relation_to<R>(self, target: EntityId) -> QueryRef<'a, Q, (WithRelationTo<R>, F)>
    where
        R: Relation,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query,
            filter: (WithRelationTo::new(target), self.filter),
        }
    }

    /// Adds query to fetch modified components.
    #[inline]
    pub fn modified<T>(self, after_epoch: EpochId) -> QueryRef<'a, TuplePlus<Q, Modified<T>>, F>
    where
        Modified<T>: Query,
        Q: ExtendTuple<Modified<T>>,
        Q::Query: ExtendTuple<Modified<T>>,
        TuplePlus<Q, Modified<T>>: IntoQuery<Query = TuplePlus<Q::Query, Modified<T>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query.extend_tuple(Modified::new(after_epoch)),
            filter: self.filter,
        }
    }

    /// Extends query to borrow from components.
    #[inline]
    pub fn borrow_any<T>(self) -> QueryRef<'a, TuplePlus<Q, QueryBorrowAny<T>>, F>
    where
        QueryBorrowAny<T>: PhantomQuery,
        Q: ExtendTuple<QueryBorrowAny<T>>,
        Q::Query: ExtendTuple<PhantomData<QueryBorrowAny<T>>>,
        TuplePlus<Q, QueryBorrowAny<T>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<QueryBorrowAny<T>>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData),
            filter: self.filter,
        }
    }

    /// Extends query to borrow from components.
    #[inline]
    pub fn borrow_one<T>(self, id: TypeId) -> QueryRef<'a, TuplePlus<Q, QueryBorrowOne<T>>, F>
    where
        QueryBorrowOne<T>: Query,
        Q: ExtendTuple<QueryBorrowOne<T>>,
        Q::Query: ExtendTuple<QueryBorrowOne<T>>,
        TuplePlus<Q, QueryBorrowOne<T>>: IntoQuery<Query = TuplePlus<Q::Query, QueryBorrowOne<T>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query.extend_tuple(QueryBorrowOne::new(id)),
            filter: self.filter,
        }
    }

    /// Extends query to borrow from components.
    #[inline]
    pub fn borrow_all<T>(self) -> QueryRef<'a, TuplePlus<Q, QueryBorrowAll<T>>, F>
    where
        QueryBorrowAll<T>: PhantomQuery,
        Q: ExtendTuple<QueryBorrowAll<T>>,
        Q::Query: ExtendTuple<PhantomData<QueryBorrowAll<T>>>,
        TuplePlus<Q, QueryBorrowAll<T>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<QueryBorrowAll<T>>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    #[inline]
    pub fn relation<R>(self) -> QueryRef<'a, TuplePlus<Q, QueryRelation<R>>, F>
    where
        QueryRelation<R>: PhantomQuery,
        Q: ExtendTuple<QueryRelation<R>>,
        Q::Query: ExtendTuple<PhantomData<QueryRelation<R>>>,
        TuplePlus<Q, QueryRelation<R>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<QueryRelation<R>>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    #[inline]
    pub fn relation_to<R>(
        self,
        entity: EntityId,
    ) -> QueryRef<'a, TuplePlus<Q, QueryRelationTo<R>>, F>
    where
        QueryRelationTo<R>: Query,
        Q: ExtendTuple<QueryRelationTo<R>>,
        Q::Query: ExtendTuple<QueryRelationTo<R>>,
        TuplePlus<Q, QueryRelationTo<R>>:
            IntoQuery<Query = TuplePlus<Q::Query, QueryRelationTo<R>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query.extend_tuple(QueryRelationTo::new(entity)),
            filter: self.filter,
        }
    }

    /// Adds query to fetch relation.
    #[inline]
    pub fn related<R>(self) -> QueryRef<'a, TuplePlus<Q, QueryRelated<R>>, F>
    where
        QueryRelated<R>: PhantomQuery,
        Q: ExtendTuple<QueryRelated<R>>,
        Q::Query: ExtendTuple<PhantomData<QueryRelated<R>>>,
        TuplePlus<Q, QueryRelated<R>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<QueryRelated<R>>>>,
    {
        QueryRef {
            archetypes: self.archetypes,
            entities: self.entities,
            epoch: self.epoch,
            query: self.query.extend_tuple(PhantomData),
            filter: self.filter,
        }
    }
}

impl<'a, Q, F> QueryRef<'a, Q, F>
where
    Q: IntoQuery,
    F: IntoFilter,
{
    /// Performs query from single entity.
    pub fn one(&mut self, entity: EntityId) -> Result<QueryItem<'_, Q>, QueryOneError> {
        let epoch = self.epoch.next();

        let (archetype, idx) = self
            .entities
            .get(entity)
            .ok_or(QueryOneError::NoSuchEntity)?;

        let archetype = &self.archetypes[archetype as usize];

        debug_assert!(archetype.len() >= idx as usize, "Entity index is valid");

        if self.filter.skip_archetype(archetype) {
            return Err(QueryOneError::NotSatisfied);
        }

        if self.query.skip_archetype(archetype) {
            return Err(QueryOneError::NotSatisfied);
        }

        let mut filter_fetch = unsafe { self.filter.fetch(archetype, epoch) };
        let mut query_fetch = unsafe { self.query.fetch(archetype, epoch) };

        if unsafe { filter_fetch.skip_chunk(chunk_idx(idx as usize)) } {
            return Err(QueryOneError::NotSatisfied);
        }

        if unsafe { query_fetch.skip_chunk(chunk_idx(idx as usize)) } {
            return Err(QueryOneError::NotSatisfied);
        }

        unsafe { filter_fetch.visit_chunk(chunk_idx(idx as usize)) }
        unsafe { query_fetch.visit_chunk(chunk_idx(idx as usize)) }

        if unsafe { filter_fetch.skip_item(idx as usize) } {
            return Err(QueryOneError::NotSatisfied);
        }

        if unsafe { query_fetch.skip_item(idx as usize) } {
            return Err(QueryOneError::NotSatisfied);
        }

        unsafe { filter_fetch.get_item(idx as usize) };
        let item = unsafe { query_fetch.get_item(idx as usize) };
        Ok(item)
    }

    /// Returns iterator over immutable query results.
    /// This method is only available with non-tracking queries.
    #[inline]
    pub fn iter<'b>(&self) -> QueryIter<'a, FilteredQuery<F::Filter, Q::Query>>
    where
        Q::Query: Clone,
        F::Filter: Clone,
    {
        let epoch = self.epoch.next();

        QueryIter::new(
            FilteredQuery {
                filter: self.filter.clone(),
                query: self.query.clone(),
            },
            epoch,
            self.archetypes,
        )
    }

    /// Returns iterator over query results.
    /// This method is only available with non-tracking queries.
    #[inline]
    pub fn into_iter(self) -> QueryIter<'a, FilteredQuery<F::Filter, Q::Query>> {
        let epoch = self.epoch.next();

        QueryIter::new(
            FilteredQuery {
                filter: self.filter,
                query: self.query,
            },
            epoch,
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
        Fun: FnMut(QueryItem<'_, Q>),
    {
        let epoch = self.epoch.next();
        for_each_impl(self.filter, self.query, self.archetypes, epoch, f)
    }
}

impl<'a, Q, F> IntoIterator for QueryRef<'a, Q, F>
where
    Q: IntoQuery,
    F: IntoFilter,
{
    type Item = (EntityId, QueryItem<'a, Q>);
    type IntoIter = QueryIter<'a, FilteredQuery<F::Filter, Q::Query>>;

    fn into_iter(self) -> QueryIter<'a, FilteredQuery<F::Filter, Q::Query>> {
        self.into_iter()
    }
}

pub(crate) fn for_each_impl<Q, F, Fun>(
    filter: F,
    query: Q,
    archetypes: &[Archetype],
    epoch: EpochId,
    mut f: Fun,
) where
    Q: Query,
    F: Filter,
    Fun: FnMut(QueryItem<'_, Q>),
{
    let mut query = FilteredQuery {
        filter: filter,
        query: query,
    };

    for archetype in archetypes {
        if archetype.is_empty() {
            continue;
        }

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
