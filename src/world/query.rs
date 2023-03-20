use core::{
    any::TypeId,
    cell::Cell,
    convert::Infallible,
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

use crate::{
    archetype::{chunk_idx, first_of_chunk, Archetype, CHUNK_LEN_USIZE},
    entity::{EntityId, EntitySet},
    query::{
        Copied, Fetch, FilteredQuery, ImmutableQuery, IntoQuery, Modified, MutQuery, Not,
        PhantomQuery, Query, QueryBorrowAll, QueryBorrowAny, QueryBorrowOne, QueryItem, QueryIter,
        With, Without,
    },
    relation::{Related, Relates, RelatesExclusive, RelatesTo},
    world::QueryOneError,
};

use super::{EpochCounter, EpochId, World};

pub trait ExtendTuple<E>: Sized {
    type Output;

    fn extend_tuple(self, element: E) -> Self::Output;
}

pub type TuplePlus<T, E> = <T as ExtendTuple<E>>::Output;

macro_rules! impl_extend {
    ($($a:ident)*) => {
        impl<Add $(, $a)*> ExtendTuple<Add> for ($($a,)*)
        {
            type Output = ($($a,)* Add,);

            #[inline]
            fn extend_tuple(self, add: Add) -> Self::Output {
                #![allow(non_snake_case)]
                let ($($a,)*) = self;
                ($($a,)* add,)
            }
        }
    };
}

for_tuple!(impl_extend);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BorrowState {
    NotBorrowed,
    Borrowed,
    Unchecked,
}

use BorrowState::*;

/// Query builder.
pub struct QueryRef<'a, Q: IntoQuery, F: IntoQuery = ()> {
    archetypes: &'a [Archetype],
    entities: &'a EntitySet,
    epoch: &'a EpochCounter,
    filtered_query: FilteredQuery<F::Query, Q::Query>,
    borrowed: Cell<BorrowState>,
}

struct QueryRefParts<'a, Q: IntoQuery, F: IntoQuery> {
    archetypes: &'a [Archetype],
    entities: &'a EntitySet,
    epoch: &'a EpochCounter,
    filtered_query: FilteredQuery<F::Query, Q::Query>,
    borrowed: BorrowState,
}

impl<'a, Q, F> Drop for QueryRef<'a, Q, F>
where
    Q: IntoQuery,
    F: IntoQuery,
{
    fn drop(&mut self) {
        self.release();
    }
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
            filtered_query: FilteredQuery { filter, query },
            borrowed: Cell::new(NotBorrowed),
        }
    }

    /// Constructs query from query part, filter part and world.
    #[inline]
    pub unsafe fn new_unchecked(world: &'a World, query: Q::Query, filter: F::Query) -> Self {
        QueryRef {
            archetypes: world.archetypes(),
            entities: &world.entities,
            epoch: world.epoch_counter(),
            filtered_query: FilteredQuery { filter, query },
            borrowed: Cell::new(Unchecked),
        }
    }

    #[inline]
    fn deconstruct(self) -> QueryRefParts<'a, Q, F> {
        let mut me = ManuallyDrop::new(self);
        me.release();

        QueryRefParts {
            archetypes: me.archetypes,
            entities: me.entities,
            epoch: me.epoch,
            filtered_query: unsafe { core::ptr::read(&mut me.filtered_query) },
            borrowed: me.borrowed.get(),
        }
    }

    /// Creates new layer of tuples of mutable query.
    #[inline]
    pub fn layer(self) -> QueryRef<'a, (Q,), F> {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: (parts.filtered_query.query,),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds specified query.
    #[inline]
    pub fn extend_query<T>(self, query: T) -> QueryRef<'a, TuplePlus<Q, T::Query>, F>
    where
        T: IntoQuery,
        Q: ExtendTuple<T::Query>,
        Q::Query: ExtendTuple<T::Query>,
        TuplePlus<Q, T::Query>: IntoQuery<Query = TuplePlus<Q::Query, T::Query>>,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query.extend_tuple(query.into_query()),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds filter that skips entities that don't have specified component.
    #[inline]
    pub fn with<T>(self) -> QueryRef<'a, Q, (With<T>, F)>
    where
        T: 'static,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query,
                filter: (PhantomData, parts.filtered_query.filter),
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds filter that skips entities that have specified component.
    #[inline]
    pub fn without<T>(self) -> QueryRef<'a, Q, (Without<T>, F)>
    where
        T: 'static,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query,
                filter: (Not(PhantomData), parts.filtered_query.filter),
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds filter to the query.
    #[inline]
    pub fn filter<T>(self, filter: T) -> QueryRef<'a, Q, (T, F)>
    where
        T: ImmutableQuery,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query,
                filter: (filter, parts.filtered_query.filter),
            },
            borrowed: Cell::new(parts.borrowed),
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
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts
                    .filtered_query
                    .query
                    .extend_tuple(Modified::new(after_epoch)),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds query to fetch modified components.
    #[inline]
    pub fn filter_modified<T>(self, after_epoch: EpochId) -> QueryRef<'a, Q, (Modified<With<T>>, F)>
    where
        T: 'static,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query,
                filter: (Modified::new(after_epoch), parts.filtered_query.filter),
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds query to fetch copy of component.
    #[inline]
    pub fn copied<T>(self) -> QueryRef<'a, TuplePlus<Q, Copied<T>>, F>
    where
        Copied<T>: PhantomQuery,
        Q: ExtendTuple<Copied<T>>,
        Q::Query: ExtendTuple<PhantomData<fn() -> Copied<T>>>,
        TuplePlus<Q, Copied<T>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<fn() -> Copied<T>>>>,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query.extend_tuple(PhantomData),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Extends query to borrow from components.
    #[inline]
    pub fn borrow_any<T>(self) -> QueryRef<'a, TuplePlus<Q, QueryBorrowAny<T>>, F>
    where
        QueryBorrowAny<T>: PhantomQuery,
        Q: ExtendTuple<QueryBorrowAny<T>>,
        Q::Query: ExtendTuple<PhantomData<fn() -> QueryBorrowAny<T>>>,
        TuplePlus<Q, QueryBorrowAny<T>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<fn() -> QueryBorrowAny<T>>>>,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query.extend_tuple(PhantomData),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
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
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts
                    .filtered_query
                    .query
                    .extend_tuple(QueryBorrowOne::new(id)),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Extends query to borrow from components.
    #[inline]
    pub fn borrow_all<T>(self) -> QueryRef<'a, TuplePlus<Q, QueryBorrowAll<T>>, F>
    where
        QueryBorrowAll<T>: PhantomQuery,
        Q: ExtendTuple<QueryBorrowAll<T>>,
        Q::Query: ExtendTuple<PhantomData<fn() -> QueryBorrowAll<T>>>,
        TuplePlus<Q, QueryBorrowAll<T>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<fn() -> QueryBorrowAll<T>>>>,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query.extend_tuple(PhantomData),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds query to fetch relation.
    #[inline]
    pub fn relates<R>(self) -> QueryRef<'a, TuplePlus<Q, Relates<R>>, F>
    where
        Relates<R>: PhantomQuery,
        Q: ExtendTuple<Relates<R>>,
        Q::Query: ExtendTuple<PhantomData<fn() -> Relates<R>>>,
        TuplePlus<Q, Relates<R>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<fn() -> Relates<R>>>>,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query.extend_tuple(PhantomData),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds query to fetch relation.
    #[inline]
    pub fn relates_exclusive<R>(self) -> QueryRef<'a, TuplePlus<Q, RelatesExclusive<R>>, F>
    where
        RelatesExclusive<R>: PhantomQuery,
        Q: ExtendTuple<RelatesExclusive<R>>,
        Q::Query: ExtendTuple<PhantomData<fn() -> RelatesExclusive<R>>>,
        TuplePlus<Q, RelatesExclusive<R>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<fn() -> RelatesExclusive<R>>>>,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query.extend_tuple(PhantomData),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds query to fetch relation.
    #[inline]
    pub fn relates_to<R>(self, entity: EntityId) -> QueryRef<'a, TuplePlus<Q, RelatesTo<R>>, F>
    where
        RelatesTo<R>: Query,
        Q: ExtendTuple<RelatesTo<R>>,
        Q::Query: ExtendTuple<RelatesTo<R>>,
        TuplePlus<Q, RelatesTo<R>>: IntoQuery<Query = TuplePlus<Q::Query, RelatesTo<R>>>,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts
                    .filtered_query
                    .query
                    .extend_tuple(RelatesTo::new(entity)),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Adds query to fetch relation.
    #[inline]
    pub fn related<R>(self) -> QueryRef<'a, TuplePlus<Q, Related<R>>, F>
    where
        Related<R>: PhantomQuery,
        Q: ExtendTuple<Related<R>>,
        Q::Query: ExtendTuple<PhantomData<fn() -> Related<R>>>,
        TuplePlus<Q, Related<R>>:
            IntoQuery<Query = TuplePlus<Q::Query, PhantomData<fn() -> Related<R>>>>,
    {
        let parts = self.deconstruct();

        QueryRef {
            archetypes: parts.archetypes,
            entities: parts.entities,
            epoch: parts.epoch,
            filtered_query: FilteredQuery {
                query: parts.filtered_query.query.extend_tuple(PhantomData),
                filter: parts.filtered_query.filter,
            },
            borrowed: Cell::new(parts.borrowed),
        }
    }

    /// Borrow from archetypes
    fn ensure_borrow(&self) {
        if self.borrowed.get() != NotBorrowed {
            return;
        }

        acquire_archetypes(self.archetypes, &self.filtered_query);

        self.borrowed.set(Borrowed);
    }

    /// Release borrow locks from archetypes.
    /// Borrow locks are acquired with [`QueryRef::get_one`], [`QueryRef::iter`] and [`QueryRef::iter_mut`] methods.
    /// Borrow locks are automatically released when the [`QueryRef`] is dropped.
    ///
    /// This method allows to release borrows early and reuse the query later.
    /// For example in system with conflicting queries it is possible
    /// to use this method to release borrows from one query and then use another query.
    pub fn release(&mut self) {
        if *self.borrowed.get_mut() == Borrowed {
            return;
        }

        release_archetypes(self.archetypes, &self.filtered_query);
        *self.borrowed.get_mut() = NotBorrowed;
    }
}

impl<'a, Q, F> QueryRef<'a, Q, F>
where
    Q: IntoQuery,
    F: IntoQuery,
    F::Query: ImmutableQuery,
{
    /// Queries components from specified entity.
    /// Returns query item for the entity or error.
    ///
    /// Locks all archetypes for the query.
    pub fn get_one(
        &mut self,
        entity: EntityId,
    ) -> Result<QueryItem<'_, FilteredQuery<F::Query, Q::Query>>, QueryOneError> {
        let (archetype, idx) = self
            .entities
            .get_location(entity)
            .ok_or(QueryOneError::NoSuchEntity)?;

        let archetype = &self.archetypes[archetype as usize];

        debug_assert!(archetype.len() >= idx as usize, "Entity index is valid");

        if !self.filtered_query.visit_archetype(archetype) {
            return Err(QueryOneError::NotSatisfied);
        }

        self.ensure_borrow();

        let epoch = self.epoch.next();

        let mut fetch = unsafe { self.filtered_query.fetch(archetype, epoch) };

        if !unsafe { fetch.visit_chunk(chunk_idx(idx as usize)) } {
            return Err(QueryOneError::NotSatisfied);
        }

        if !unsafe { fetch.visit_item(idx as usize) } {
            return Err(QueryOneError::NotSatisfied);
        }

        unsafe { fetch.touch_chunk(chunk_idx(idx as usize)) }

        let item = unsafe { fetch.get_item(idx as usize) };

        Ok(item)
    }

    /// Queries components from specified entity.
    /// Calls provided closure with query item and its result or error.
    ///
    /// This method does not allow references from item to escape the closure.
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn for_one<Fun, R>(&mut self, entity: EntityId, f: Fun) -> Result<R, QueryOneError>
    where
        for<'b> Fun: FnOnce(QueryItem<'b, FilteredQuery<F::Query, Q::Query>>) -> R,
    {
        let epoch = self.epoch.next();

        if self.borrowed.get() != BorrowState::NotBorrowed {
            for_one_pre_borrowed(
                MutQuery::new(&mut self.filtered_query),
                self.entities,
                self.archetypes,
                epoch,
                entity,
                f,
            )
        } else {
            for_one(
                MutQuery::new(&mut self.filtered_query),
                self.entities,
                self.archetypes,
                epoch,
                entity,
                f,
            )
        }
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_one_owned<T>(&mut self, entity: EntityId) -> Result<T::Owned, QueryOneError>
    where
        T: ToOwned + 'static,
        Q::Query: for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one(entity, |item| T::to_owned(item))
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`Clone`].
    /// Returns cloned item value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_one_cloned<T>(&mut self, entity: EntityId) -> Result<T, QueryOneError>
    where
        T: Clone + 'static,
        Q::Query: for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one(entity, |item| T::clone(item))
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`Copy`].
    /// Returns copied item value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_one_copied<T>(&mut self, entity: EntityId) -> Result<T, QueryOneError>
    where
        T: Copy + 'static,
        Q::Query: for<'b> Query<Item<'b> = &'b T>,
    {
        self.for_one(entity, |item| *item)
    }

    /// Returns iterator over query results.
    ///
    /// Returned iterator borrows lifetime from this [`QueryRef`] instance.
    #[inline]
    pub fn iter(&self) -> QueryIter<'_, FilteredQuery<F::Query, Q::Query>>
    where
        Q::Query: ImmutableQuery + Clone,
        F::Query: Clone,
    {
        self.ensure_borrow();

        let epoch = self.epoch.next();

        QueryIter::new(self.filtered_query.clone(), epoch, self.archetypes)
    }

    /// Returns iterator over query results.
    ///
    /// Returned iterator borrows lifetime from this [`QueryRef`] instance.
    #[inline]
    pub fn iter_mut(&mut self) -> QueryIter<'_, MutQuery<FilteredQuery<F::Query, Q::Query>>> {
        self.ensure_borrow();

        let epoch = self.epoch.next();

        QueryIter::new(
            MutQuery::new(&mut self.filtered_query),
            epoch,
            self.archetypes,
        )
    }

    /// Calls a closure on each query item.
    ///
    /// This method does not allow references from items to escape the closure.
    /// This allows it to lock only archetype which is currently iterated.
    /// Yet this method won't release borrow locks if they are already acquired.
    ///
    /// For example if `Option<Component>` is used in query,
    /// and closure receives `None` for some entity,
    /// A query with `&mut Component` can be used inside the closure.
    /// This is not possible with iterator returned by `QueryRef` and `Iterator::for_each` method.
    #[inline]
    pub fn for_each<Fun>(&mut self, mut f: Fun)
    where
        Fun: for<'b> FnMut(QueryItem<'b, Q>),
    {
        self.fold((), move |(), item| f(item));
    }

    /// Calls a closure on each query item.
    /// Breaks when closure returns `Err` and returns that value.
    ///
    /// This method does not allow references from items to escape the closure.
    /// This allows it to lock only archetype which is currently iterated.
    /// Yet this method won't release borrow locks if they are already acquired.
    ///
    /// For example if `Option<Component>` is used in query,
    /// and closure receives `None` for some entity,
    /// A query with `&mut Component` can be used inside the closure.
    /// This is not possible with iterator returned by `QueryRef` and `Iterator::for_each` method.
    #[inline]
    pub fn try_for_each<E, Fun>(&mut self, mut f: Fun) -> Result<(), E>
    where
        Fun: for<'b> FnMut(QueryItem<'b, Q>) -> Result<(), E>,
    {
        self.try_fold((), move |(), item| f(item))
    }

    /// Folds every query item into an accumulator by applying an operation, returning the final result.
    ///
    /// This method does not allow references from items to escape the closure.
    /// This allows it to lock only archetype which is currently iterated for the duration of the closure call.
    /// Yet this method won't release borrow locks if they are already acquired.
    ///
    /// For example if `Option<Component>` is used in query,
    /// and closure receives `None` for some entity,
    /// A query with `&mut Component` can be used inside the closure.
    /// This is not possible with iterator returned by `QueryRef` and `Iterator::for_each` method.
    #[inline]
    pub fn fold<T, Fun>(&mut self, acc: T, mut f: Fun) -> T
    where
        Fun: for<'b> FnMut(T, QueryItem<'b, Q>) -> T,
    {
        let res = self.try_fold(acc, |acc, item| Ok::<_, Infallible>(f(acc, item)));

        match res {
            Ok(acc) => acc,
            Err(infallible) => match infallible {},
        }
    }

    /// Folds every query item into an accumulator by applying an operation, returning the final result.
    /// Breaks when closure returns `Err` and returns that value.
    ///
    /// This method does not allow references from items to escape the closure.
    /// This allows it to lock only archetype which is currently iterated for the duration of the closure call.
    /// Yet this method won't release borrow locks if they are already acquired.
    ///
    /// For example if `Option<Component>` is used in query,
    /// and closure receives `None` for some entity,
    /// A query with `&mut Component` can be used inside the closure.
    /// This is not possible with iterator returned by `QueryRef` and `Iterator::for_each` method.
    #[inline]
    pub fn try_fold<T, E, Fun>(&mut self, acc: T, f: Fun) -> Result<T, E>
    where
        Fun: for<'b> FnMut(T, QueryItem<'b, Q>) -> Result<T, E>,
    {
        let epoch = self.epoch.next();

        try_fold(
            MutQuery::new(&mut self.filtered_query),
            self.archetypes,
            epoch,
            self.borrowed.get() != BorrowState::NotBorrowed,
            acc,
            f,
        )
    }
}

impl<'a, Q, F> IntoIterator for &'a mut QueryRef<'_, Q, F>
where
    Q: IntoQuery,
    F: IntoQuery,
    F::Query: ImmutableQuery,
{
    type Item = QueryItem<'a, Q>;
    type IntoIter = QueryIter<'a, MutQuery<'a, FilteredQuery<F::Query, Q::Query>>>;

    #[inline]
    fn into_iter(self) -> QueryIter<'a, MutQuery<'a, FilteredQuery<F::Query, Q::Query>>> {
        self.iter_mut()
    }
}

impl<'a, Q, F> IntoIterator for &'a QueryRef<'_, Q, F>
where
    Q: IntoQuery,
    Q::Query: ImmutableQuery + Clone,
    F: IntoQuery,
    F::Query: ImmutableQuery + Clone,
{
    type Item = QueryItem<'a, Q>;
    type IntoIter = QueryIter<'a, FilteredQuery<F::Query, Q::Query>>;

    #[inline]
    fn into_iter(self) -> QueryIter<'a, FilteredQuery<F::Query, Q::Query>> {
        self.iter()
    }
}

fn for_one<Q, R, Fun>(
    mut query: Q,
    entities: &EntitySet,
    archetypes: &[Archetype],
    epoch: EpochId,
    id: EntityId,
    f: Fun,
) -> Result<R, QueryOneError>
where
    Q: Query,
    Fun: for<'a> FnOnce(QueryItem<'a, Q>) -> R,
{
    let (archetype_idx, idx) = entities
        .get_location(id)
        .ok_or(QueryOneError::NoSuchEntity)?;

    let archetype_idx = archetype_idx as usize;
    let idx = idx as usize;

    let archetype = unsafe { archetypes.get_unchecked(archetype_idx) };

    if !query.visit_archetype(archetype) {
        return Err(QueryOneError::NotSatisfied);
    }

    unsafe {
        query.access_archetype(archetype, &|id, access| {
            let success = archetype.component(id).unwrap_unchecked().borrow(access);
            assert!(success, "Failed to borrow from archetype");
        });
    }

    let mut query = borrow_archetype(archetype, &mut query);

    let mut fetch = unsafe { query.fetch(archetype, epoch) };
    if !unsafe { fetch.visit_chunk(chunk_idx(idx)) } {
        return Err(QueryOneError::NotSatisfied);
    }

    if !unsafe { fetch.visit_item(idx) } {
        return Err(QueryOneError::NotSatisfied);
    }

    unsafe { fetch.touch_chunk(chunk_idx(idx)) }

    let item = unsafe { fetch.get_item(idx) };

    Ok(f(item))
}

fn for_one_pre_borrowed<Q, R, Fun>(
    mut query: Q,
    entities: &EntitySet,
    archetypes: &[Archetype],
    epoch: EpochId,
    id: EntityId,
    f: Fun,
) -> Result<R, QueryOneError>
where
    Q: Query,
    Fun: for<'a> FnOnce(QueryItem<'a, Q>) -> R,
{
    let (archetype_idx, idx) = entities
        .get_location(id)
        .ok_or(QueryOneError::NoSuchEntity)?;

    let archetype_idx = archetype_idx as usize;
    let idx = idx as usize;

    let archetype = unsafe { archetypes.get_unchecked(archetype_idx) };

    if !query.visit_archetype(archetype) {
        return Err(QueryOneError::NotSatisfied);
    }

    let mut fetch = unsafe { query.fetch(archetype, epoch) };
    if !unsafe { fetch.visit_chunk(chunk_idx(idx)) } {
        return Err(QueryOneError::NotSatisfied);
    }

    if !unsafe { fetch.visit_item(idx) } {
        return Err(QueryOneError::NotSatisfied);
    }

    unsafe { fetch.touch_chunk(chunk_idx(idx)) }

    let item = unsafe { fetch.get_item(idx) };

    Ok(f(item))
}

fn try_fold<Q, T, E, Fun>(
    query: Q,
    archetypes: &[Archetype],
    epoch: EpochId,
    borrowed: bool,
    acc: T,
    f: Fun,
) -> Result<T, E>
where
    Q: Query,
    Fun: FnMut(T, QueryItem<'_, Q>) -> Result<T, E>,
{
    if borrowed {
        try_fold_pre_borrowed_impl(query, archetypes, epoch, acc, f)
    } else {
        try_fold_impl(query, archetypes, epoch, acc, f)
    }
}

fn try_fold_impl<Q, T, E, Fun>(
    mut query: Q,
    archetypes: &[Archetype],
    epoch: EpochId,
    mut acc: T,
    mut f: Fun,
) -> Result<T, E>
where
    Q: Query,
    Fun: FnMut(T, QueryItem<'_, Q>) -> Result<T, E>,
{
    for archetype in archetypes {
        if archetype.is_empty() {
            continue;
        }

        if !query.visit_archetype(archetype) {
            continue;
        }

        unsafe {
            query.access_archetype(archetype, &|id, access| {
                let success = archetype.component(id).unwrap_unchecked().borrow(access);
                assert!(success, "Failed to borrow from archetype");
            });
        }

        let mut query = borrow_archetype(archetype, &mut query);

        let mut fetch = unsafe { query.fetch(archetype, epoch) };

        let mut indices = 0..archetype.len();
        let mut touch_chunk = false;

        while let Some(idx) = indices.next() {
            if let Some(chunk_idx) = first_of_chunk(idx) {
                if !unsafe { fetch.visit_chunk(chunk_idx) } {
                    indices.nth(CHUNK_LEN_USIZE - 1);
                    continue;
                }
                touch_chunk = true;
            }
            if !unsafe { fetch.visit_item(idx) } {
                continue;
            }
            if touch_chunk {
                unsafe { fetch.touch_chunk(chunk_idx(idx)) }
                touch_chunk = false;
            }
            let item = unsafe { fetch.get_item(idx) };
            acc = f(acc, item)?;
        }
    }
    Ok(acc)
}

fn try_fold_pre_borrowed_impl<Q, T, E, Fun>(
    mut query: Q,
    archetypes: &[Archetype],
    epoch: EpochId,
    mut acc: T,
    mut f: Fun,
) -> Result<T, E>
where
    Q: Query,
    Fun: FnMut(T, QueryItem<'_, Q>) -> Result<T, E>,
{
    for archetype in archetypes {
        if archetype.is_empty() {
            continue;
        }

        if !query.visit_archetype(archetype) {
            continue;
        }

        let mut fetch = unsafe { query.fetch(archetype, epoch) };

        let mut indices = 0..archetype.len();
        let mut touch_chunk = false;

        while let Some(idx) = indices.next() {
            if let Some(chunk_idx) = first_of_chunk(idx) {
                if !unsafe { fetch.visit_chunk(chunk_idx) } {
                    indices.nth(CHUNK_LEN_USIZE - 1);
                    continue;
                }
                touch_chunk = true;
            }
            if !unsafe { fetch.visit_item(idx) } {
                continue;
            }
            if touch_chunk {
                unsafe { fetch.touch_chunk(chunk_idx(idx)) }
                touch_chunk = false;
            }
            let item = unsafe { fetch.get_item(idx) };
            acc = f(acc, item)?;
        }
    }
    Ok(acc)
}

/// Result for [`World::query_one`] and [`World::query_one_with`] methods.
pub struct QueryOne<'a, Q: IntoQuery> {
    query: Q::Query,
    archetype: &'a Archetype,
    idx: u32,
    epoch: &'a EpochCounter,
    borrowed: Cell<bool>,
}

impl<'a, Q> QueryOne<'a, Q>
where
    Q: IntoQuery,
{
    pub(crate) fn new(
        query: Q::Query,
        archetype: &'a Archetype,
        idx: u32,
        epoch: &'a EpochCounter,
    ) -> Self {
        QueryOne {
            query: query.into_query(),
            archetype,
            idx,
            epoch,
            borrowed: Cell::new(false),
        }
    }

    /// Borrow from archetypes
    fn ensure_borrow(&self) {
        if self.borrowed.get() {
            return;
        }

        acquire_archetypes(core::slice::from_ref(self.archetype), &self.query);

        self.borrowed.set(true);
    }

    /// Release borrow locks from archetypes.
    /// Borrow locks are acquired with [`QueryRef::get_one`], [`QueryRef::iter`] and [`QueryRef::iter_mut`] methods.
    /// Borrow locks are automatically released when the [`QueryRef`] is dropped.
    ///
    /// This method allows to release borrows early and reuse the query later.
    /// For example in system with conflicting queries it is possible
    /// to use this method to release borrows from one query and then use another query.
    pub fn release(&mut self) {
        if !self.borrowed.get() {
            return;
        }

        release_archetypes(core::slice::from_ref(self.archetype), &self.query);

        self.borrowed.set(false);
    }

    /// Runs the query on the entity.
    /// Returns query item if query is satisfied.
    pub fn get(&mut self) -> Option<QueryItem<'_, Q>> {
        let epoch = self.epoch.next();

        if !self.query.visit_archetype(self.archetype) {
            return None;
        }

        self.ensure_borrow();

        let mut fetch = unsafe { self.query.fetch(self.archetype, epoch) };

        if !unsafe { fetch.visit_chunk(chunk_idx(self.idx as usize)) } {
            return None;
        }

        if !unsafe { fetch.visit_item(self.idx as usize) } {
            return None;
        }

        unsafe { fetch.touch_chunk(chunk_idx(self.idx as usize)) }

        let item = unsafe { fetch.get_item(self.idx as usize) };
        Some(item)
    }
}

impl<'a, Q> Drop for QueryOne<'a, Q>
where
    Q: IntoQuery,
{
    #[inline]
    fn drop(&mut self) {
        if *self.borrowed.get_mut() {
            release_archetypes(core::slice::from_ref(self.archetype), &self.query);
        }
    }
}

fn acquire_archetypes(archetypes: &[Archetype], query: &impl Query) {
    struct ReleaseOnFailure<'a, Q: Query> {
        archetypes: &'a [Archetype],
        query: &'a Q,
        len: usize,
    }

    impl<'a, Q> Drop for ReleaseOnFailure<'a, Q>
    where
        Q: Query,
    {
        fn drop(&mut self) {
            for archetype in &self.archetypes[..self.len] {
                unsafe {
                    if self.query.visit_archetype(archetype) {
                        self.query.access_archetype(archetype, &|id, access| {
                            archetype.component(id).unwrap_unchecked().release(access);
                        });
                    }
                }
            }
        }
    }

    let mut guard = ReleaseOnFailure {
        archetypes,
        query,
        len: 0,
    };

    for archetype in archetypes {
        unsafe {
            if query.visit_archetype(archetype) {
                query.access_archetype(archetype, &|id, access| {
                    let success = archetype.component(id).unwrap_unchecked().borrow(access);
                    assert!(success, "Failed to lock '{:?}' from archetype", id);
                });
            }
        }
        guard.len += 1;
    }

    core::mem::forget(guard);
}

fn release_archetypes(archetypes: &[Archetype], query: &impl Query) {
    for archetype in archetypes {
        unsafe {
            if query.visit_archetype(archetype) {
                query.access_archetype(archetype, &|id, access| {
                    archetype.component(id).unwrap_unchecked().release(access);
                });
            }
        }
    }
}

pub(crate) fn borrow_archetype<'a, Q>(
    archetype: &'a Archetype,
    query: &'a mut Q,
) -> impl DerefMut<Target = Q> + 'a
where
    Q: Query,
{
    struct QueryRelease<'a, Q: Query> {
        query: &'a mut Q,
        archetype: &'a Archetype,
    }

    impl<Q> Deref for QueryRelease<'_, Q>
    where
        Q: Query,
    {
        type Target = Q;

        fn deref(&self) -> &Q {
            &*self.query
        }
    }

    impl<Q> DerefMut for QueryRelease<'_, Q>
    where
        Q: Query,
    {
        fn deref_mut(&mut self) -> &mut Q {
            &mut *self.query
        }
    }

    impl<Q> Drop for QueryRelease<'_, Q>
    where
        Q: Query,
    {
        #[inline]
        fn drop(&mut self) {
            unsafe {
                self.query.access_archetype(self.archetype, &|id, access| {
                    self.archetype
                        .component(id)
                        .unwrap_unchecked()
                        .release(access);
                });
            }
        }
    }

    QueryRelease { query, archetype }
}
