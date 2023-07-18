use core::any::TypeId;

use crate::{
    entity::Entity,
    epoch::EpochId,
    query::{
        IntoQuery, Modified, Not, Query, QueryBorrowAll, QueryBorrowAny, QueryBorrowOne, With,
        Without,
    },
    relation::{
        ExclusiveRelation, FilterRelated, FilterRelatedBy, FilterRelates, FilterRelatesTo, Related,
        Relates, RelatesExclusive, RelatesTo, Relation,
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
    ) -> ViewValue<'a, TupleQueryAdd<Q, Modified<&'a T>>, F, B>
    where
        T: Sync + 'static,
    {
        self.extend(Modified::<&T>::new(after_epoch))
    }

    /// Extends query tuple with a query element that fetches the component,
    /// filtering entities with the component and it was modified after the `after_epoch`.
    #[inline(always)]
    pub fn modified_mut<T>(
        self,
        after_epoch: EpochId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Modified<&'a mut T>>, F, B>
    where
        T: Send + 'static,
    {
        self.extend(Modified::<&mut T>::new(after_epoch))
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// First component of entity that provide `T` borrowing is used.
    /// If no component provides `T` borrowing, the entity is filtered out.
    #[inline(always)]
    pub fn borrow_any<T>(self) -> ViewValue<'a, TupleQueryAdd<Q, QueryBorrowAny<&'a T>>, F, B>
    where
        T: Sync + ?Sized + 'static,
    {
        self.extend(QueryBorrowAny::<&T>.into_query())
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// First component of entity that provide `T` borrowing is used.
    /// If no component provides `T` borrowing, the entity is filtered out.
    #[inline(always)]
    pub fn borrow_any_mut<T>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, QueryBorrowAny<&'a mut T>>, F, B>
    where
        T: Send + ?Sized + 'static,
    {
        self.extend(QueryBorrowAny::<&mut T>.into_query())
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
        ty: TypeId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, QueryBorrowOne<&'a T>>, F, B>
    where
        T: Sync + ?Sized + 'static,
    {
        self.extend(QueryBorrowOne::<&T>::new(ty))
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
    pub fn borrow_one_mut<T>(
        self,
        ty: TypeId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, QueryBorrowOne<&'a mut T>>, F, B>
    where
        T: Send + ?Sized + 'static,
    {
        self.extend(QueryBorrowOne::<&mut T>::new(ty))
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
    pub fn borrow_all<T>(self) -> ViewValue<'a, TupleQueryAdd<Q, QueryBorrowAll<&'a T>>, F, B>
    where
        T: Sync + ?Sized + 'static,
    {
        self.extend(QueryBorrowAll::<&T>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain shared reference of the relation value
    /// and the target entity.
    #[inline(always)]
    pub fn relates<R: Relation>(self) -> ViewValue<'a, TupleQueryAdd<Q, Relates<&'a R>>, F, B> {
        self.extend(Relates::<&R>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain mutable reference of the relation value
    /// and the target entity.
    #[inline(always)]
    pub fn relates_mut<R: Relation>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Relates<&'a mut R>>, F, B> {
        self.extend(Relates::<&mut R>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain shared reference of the relation value
    /// and the target entity.
    #[inline(always)]
    pub fn relates_to<R: Relation>(
        self,
        target: impl Entity,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesTo<&'a R>>, F, B> {
        self.extend(RelatesTo::<&R>::new(target.id()))
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain mutable reference of the relation value
    /// and the target entity.
    #[inline(always)]
    pub fn relates_to_mut<R: Relation>(
        self,
        target: impl Entity,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesTo<&'a mut R>>, F, B> {
        self.extend(RelatesTo::<&mut R>::new(target.id()))
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain shared reference of the relation value
    /// and the target entity.
    #[inline(always)]
    pub fn relates_exclusive<R: ExclusiveRelation>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesExclusive<&'a R>>, F, B> {
        self.extend(RelatesExclusive::<&R>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain mutable reference of the relation value
    /// and the target entity.
    #[inline(always)]
    pub fn relates_exclusive_mut<R: ExclusiveRelation>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesExclusive<&'a mut R>>, F, B> {
        self.extend(RelatesExclusive::<&mut R>)
    }

    /// Queries for target entities in relation of type `R`.
    /// The view will contain origins of the relation.
    #[inline(always)]
    pub fn related<R: Relation>(self) -> ViewValue<'a, TupleQueryAdd<Q, Related<R>>, F, B> {
        self.extend(Related)
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
    pub fn filter<E>(self, filter: E) -> ViewValue<'a, Q, TupleQueryAdd<F, E>, B>
    where
        E: IntoQuery,
    {
        ViewValue {
            query: self.query,
            filter: F::extend_query(self.filter, filter.into_query()),
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
        self.filter(With)
    }

    /// Extends filter tuple with a filter element that\
    /// filters entities that do not have the component.
    #[inline(always)]
    pub fn without<T>(self) -> ViewValue<'a, Q, TupleQueryAdd<F, Without<T>>, B>
    where
        T: 'static,
    {
        self.filter(Not(With))
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
        self.filter(Modified::<With<T>>::new(after_epoch))
    }

    /// Filters target entities in relation of type `R`.
    #[inline(always)]
    pub fn filter_related<R: Relation>(
        self,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, FilterRelated<R>>, B> {
        self.filter(FilterRelated)
    }

    /// Filters target entities in relation of type `R`
    /// with specified origin entity.
    #[inline(always)]
    pub fn filter_related_by<R: Relation>(
        self,
        origin: impl Entity,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, FilterRelatedBy<R>>, B> {
        self.filter(FilterRelatedBy::new(origin.id()))
    }

    /// Filters origin entities in relation of type `R`.
    #[inline(always)]
    pub fn filter_relates<R: Relation>(
        self,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, FilterRelates<R>>, B> {
        self.filter(FilterRelates)
    }

    /// Filters origin entities in relation of type `R`
    /// with specified target entity.
    #[inline(always)]
    pub fn filter_relates_to<R: Relation>(
        self,
        target: impl Entity,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, FilterRelatesTo<R>>, B> {
        self.filter(FilterRelatesTo::new(target.id()))
    }
}
