use core::any::TypeId;

use crate::{
    epoch::EpochId,
    query::{
        DefaultQuery, IntoQuery, Modified, Not, Query, QueryBorrowAll, QueryBorrowAny,
        QueryBorrowOne, With, Without,
    },
};

use super::{BorrowState, ViewValue};

/// A helper trait to extend tuples of queries to produce a new query.
pub trait TupleQuery: Query + Sized {
    /// Tuple query with an additional element `E`.
    type Extended<E: Query>: Query;

    /// Extend tuple with an additional element `E`.
    fn extend_query<E: Query>(self, element: E) -> Self::Extended<E>;
}

/// A helper type alias to extend tuples.
pub type TupleQueryAdd<T, E> = <T as TupleQuery>::Extended<<E as IntoQuery>::Query>;

macro_rules! impl_extend {
    () => {};
    ($skip:ident) => {
        impl TupleQuery for ()
        {
            type Extended<Add: Query> = (Add,);

            #[inline(always)]
            fn extend_query<Add: Query>(self, add: Add) -> (Add,) {
                (add,)
            }
        }
    };
    ($skip:ident $($a:ident)+) => {
        impl<$($a),*> TupleQuery for ($($a,)*)
        where
            $($a: Query,)*
        {
            type Extended<Add: Query> = ($($a,)* Add,);

            #[inline(always)]
            fn extend_query<Add: Query>(self, add: Add) -> Self::Extended<Add> {
                #![allow(non_snake_case)]
                let ($($a,)*) = self;
                ($($a,)* add,)
            }
        }
    };
}

for_tuple!(impl_extend);

impl<'a, Q, F, B> ViewValue<'a, Q, F, B>
where
    Q: TupleQuery,
    F: Query,
    B: BorrowState,
{
    /// Extends query tuple with an additional query element.
    #[inline(always)]
    pub fn extend<E>(self, query: E) -> ViewValue<'a, TupleQueryAdd<Q, E>, F, B>
    where
        E: IntoQuery,
    {
        ViewValue {
            query: Q::extend_query(self.query, query.into_query()),
            filter: self.filter,
            archetypes: self.archetypes,
            entity_set: self.entity_set,
            borrow: self.borrow,
            epochs: self.epochs,
        }
    }

    /// Extends query tuple with a query element that fetches the component,
    /// filtering entities with the component and it was modified after the `after_epoch`.
    #[inline(always)]
    pub fn modified<T>(
        self,
        after_epoch: EpochId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Modified<T>>, F, B>
    where
        T: DefaultQuery,
        Modified<T>: Query,
    {
        self.extend(Modified::new(after_epoch))
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// First component of entity that provide `T` borrowing is used.
    /// If no component provides `T` borrowing, the entity is filtered out.
    #[inline(always)]
    pub fn borrow_any<T>(self) -> ViewValue<'a, TupleQueryAdd<Q, QueryBorrowAny<T>>, F, B>
    where
        QueryBorrowAny<T>: Query,
    {
        self.extend(QueryBorrowAny)
    }
    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// Component with user-specified `TypeId` is used.
    /// If component with the `TypeId` is not found, the entity is filtered out.
    ///
    /// # Panicking
    ///
    /// If component with the `TypeId` does not provide `T` borrowing, it panics.
    #[inline(always)]
    pub fn borrow_one<T>(
        self,
        id: TypeId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, QueryBorrowOne<T>>, F, B>
    where
        QueryBorrowOne<T>: Query,
    {
        self.extend(QueryBorrowOne::new(id))
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// Component with user-specified `TypeId` is used.
    /// If component with the `TypeId` is not found, the entity is filtered out.
    ///
    /// # Panicking
    ///
    /// If component with the `TypeId` does not provide `T` borrowing, it panics.
    #[inline(always)]
    pub fn borrow_all<T>(self) -> ViewValue<'a, TupleQueryAdd<Q, QueryBorrowAll<T>>, F, B>
    where
        QueryBorrowAll<T>: Query,
    {
        self.extend(QueryBorrowAll)
    }
}

impl<'a, Q, F, B> ViewValue<'a, Q, F, B>
where
    Q: Query,
    F: TupleQuery,
    B: BorrowState,
{
    /// Extends filter tuple with an additional filter element.
    #[inline(always)]
    pub fn filter<E>(self, filter: E::Query) -> ViewValue<'a, Q, TupleQueryAdd<F, E>, B>
    where
        E: IntoQuery,
    {
        ViewValue {
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
    #[inline(always)]
    pub fn with<T>(self) -> ViewValue<'a, Q, TupleQueryAdd<F, With<T>>, B>
    where
        T: 'static,
    {
        self.filter::<With<T>>(With)
    }

    /// Extends filter tuple with a filter element that\
    /// filters entities that do not have the component.
    #[inline(always)]
    pub fn without<T>(self) -> ViewValue<'a, Q, TupleQueryAdd<F, Without<T>>, B>
    where
        T: 'static,
    {
        self.filter::<Without<T>>(Not(With))
    }

    /// Extends filter tuple with a filter element that
    /// filters entities that have the component and it was modified after the `after_epoch`.
    #[inline(always)]
    pub fn filter_modified<T>(
        self,
        after_epoch: EpochId,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, Modified<With<T>>>, B>
    where
        T: 'static,
    {
        self.filter::<Modified<With<T>>>(Modified::new(after_epoch))
    }
}
