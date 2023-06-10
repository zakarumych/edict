use core::{any::TypeId, marker::PhantomData};

use crate::{
    epoch::EpochId,
    query::{
        IntoQuery, Modified, Not, PhantomQuery, Query, QueryBorrowAll, QueryBorrowAny,
        QueryBorrowOne, With, Without,
    },
};

use super::{BorrowState, View, ViewState};

/// A helper trait to extend tuples.
pub trait ExtendTupleQuery<E: IntoQuery>: IntoQuery + Sized {
    /// Tuple query with an additional element `E`.
    type Output: IntoQuery;

    /// Extend tuple with an additional element `E`.
    fn extend_query(tuple: Self::Query, element: E::Query) -> <Self::Output as IntoQuery>::Query;
}

/// A helper type alias to extend tuples.
pub type TuplePlus<T, E> = <T as ExtendTupleQuery<E>>::Output;

macro_rules! impl_extend {
    () => {};
    ($skip:ident $($a:ident)*) => {
        impl<Add $(, $a)*> ExtendTupleQuery<Add> for ($($a,)*)
        where
            $($a: IntoQuery,)*
            Add: IntoQuery,
        {
            type Output = ($($a,)* Add,);

            #[inline(always)]
            fn extend_query(tuple: ($($a::Query,)*), add: Add::Query) -> <Self::Output as IntoQuery>::Query {
                #![allow(non_snake_case)]
                let ($($a,)*) = tuple;
                ($($a,)* add,)
            }
        }
    };
}

for_tuple!(impl_extend);

impl<'a, Q, F, B> ViewState<'a, Q, F, B>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    /// Extends query tuple with an additional query element.
    #[inline]
    pub fn extend_query<E>(self, query: E::Query) -> ViewState<'a, TuplePlus<Q, E>, F, B>
    where
        E: IntoQuery,
        Q: ExtendTupleQuery<E>,
    {
        View {
            query: Q::extend_query(self.query, query),
            filter: self.filter,
            archetypes: self.archetypes,
            entity_set: self.entity_set,
            borrow: self.borrow,
            epochs: self.epochs,
        }
    }

    /// Extends filter tuple with an additional filter element.
    #[inline]
    pub fn extend_filter<E>(self, filter: E::Query) -> View<'a, Q, TuplePlus<F, E>, B>
    where
        E: IntoQuery,
        F: ExtendTupleQuery<E>,
    {
        View {
            query: self.query,
            filter: F::extend_query(self.filter, filter),
            archetypes: self.archetypes,
            entity_set: self.entity_set,
            borrow: self.borrow,
            epochs: self.epochs,
        }
    }

    /// Extends filter tuple with a filter element that
    /// filters entities that have the component.
    #[inline]
    pub fn with<T>(self) -> View<'a, Q, TuplePlus<F, With<T>>, B>
    where
        T: 'static,
        F: ExtendTupleQuery<With<T>>,
    {
        self.extend_filter(PhantomData)
    }

    /// Extends filter tuple with a filter element that\
    /// filters entities that do not have the component.
    #[inline]
    pub fn without<T>(self) -> View<'a, Q, TuplePlus<F, Without<T>>, B>
    where
        T: 'static,
        F: ExtendTupleQuery<Without<T>>,
    {
        self.extend_filter(Not(PhantomData))
    }

    /// Extends query tuple with a query element that fetches the component,
    /// filtering entities with the component and it was modified after the `after_epoch`.
    #[inline]
    pub fn modified<T>(self, after_epoch: EpochId) -> View<'a, TuplePlus<Q, Modified<T>>, F, B>
    where
        Modified<T>: Query,
        Q: ExtendTupleQuery<Modified<T>>,
    {
        self.extend_query(Modified::new(after_epoch))
    }

    /// Extends filter tuple with a filter element that
    /// filters entities that have the component and it was modified after the `after_epoch`.
    #[inline]
    pub fn filter_modified<T>(
        self,
        after_epoch: EpochId,
    ) -> View<'a, Q, TuplePlus<F, Modified<With<T>>>, B>
    where
        T: 'static,
        F: ExtendTupleQuery<Modified<With<T>>>,
    {
        self.extend_filter(Modified::new(after_epoch))
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// First component of entity that provide `T` borrowing is used.
    /// If no component provides `T` borrowing, the entity is filtered out.
    #[inline]
    pub fn borrow_any<T>(self) -> View<'a, TuplePlus<Q, QueryBorrowAny<T>>, F, B>
    where
        QueryBorrowAny<T>: PhantomQuery,
        Q: ExtendTupleQuery<QueryBorrowAny<T>>,
    {
        self.extend_query(PhantomData)
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// Component with user-specified `TypeId` is used.
    /// If component with the `TypeId` is not found, the entity is filtered out.
    ///
    /// # Panicking
    ///
    /// If component with the `TypeId` does not provide `T` borrowing, it panics.
    #[inline]
    pub fn borrow_one<T>(self, id: TypeId) -> View<'a, TuplePlus<Q, QueryBorrowOne<T>>, F, B>
    where
        QueryBorrowOne<T>: Query,
        Q: ExtendTupleQuery<QueryBorrowOne<T>>,
    {
        self.extend_query(QueryBorrowOne::new(id))
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// Component with user-specified `TypeId` is used.
    /// If component with the `TypeId` is not found, the entity is filtered out.
    ///
    /// # Panicking
    ///
    /// If component with the `TypeId` does not provide `T` borrowing, it panics.
    #[inline]
    pub fn borrow_all<T>(self) -> View<'a, TuplePlus<Q, QueryBorrowAll<T>>, F, B>
    where
        QueryBorrowAll<T>: PhantomQuery,
        Q: ExtendTupleQuery<QueryBorrowAll<T>>,
    {
        self.extend_query(PhantomData)
    }
}
