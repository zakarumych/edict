use core::any::TypeId;

use crate::{
    entity::Entity,
    epoch::EpochId,
    query::{
        AsQuery, BorrowAll, BorrowAny, BorrowOne, Modified, Not, Query, Read, SendQuery, With,
        Without, Write,
    },
    relation::{
        ExclusiveRelation, FilterRelated, FilterRelatedBy, FilterRelates, FilterRelatesTo, Related,
        Relates, RelatesExclusive, RelatesTo, Relation,
    },
};

use super::{BorrowState, Extensible, ViewValue};

/// A helper trait to extend tuples of queries to produce a new query.
pub trait TupleQuery: Query + Sized {
    /// Tuple query with an additional element `E`.
    type Extended<E: Query>: Query;

    /// Extend tuple with an additional element `E`.
    fn extend_query<E: Query>(self, element: E) -> Self::Extended<E>;
}

/// A helper type alias to extend tuples.
pub type TupleQueryAdd<T, E> = <T as TupleQuery>::Extended<<E as AsQuery>::Query>;

macro_rules! impl_extend {
    () => {};
    ($skip:ident) => {
        impl TupleQuery for ()
        {
            type Extended<Add: Query> = (Add,);

            #[inline]
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

            #[inline]
            fn extend_query<Add: Query>(self, add: Add) -> Self::Extended<Add> {
                #![allow(non_snake_case)]
                let ($($a,)*) = self;
                ($($a,)* add,)
            }
        }
    };
}

for_tuple!(impl_extend);

impl<'a, Q, F, B> ViewValue<'a, Q, F, B, Extensible>
where
    Q: Query,
    F: Query,
    B: BorrowState,
{
    /// Transforms view query into tuple to allow extending it with additional queries.
    ///
    /// This also helps if maximum number of queries in the tuple is reached.
    pub fn into_tuple_query(self) -> ViewValue<'a, (Q,), F, B, Extensible> {
        let query = (self.query,);
        let filter = self.filter;
        let archetypes = self.archetypes;
        let entity_set = self.entity_set;
        let epochs = self.epochs;
        let (state, Extensible) = self.extract();

        ViewValue {
            query,
            filter,
            archetypes,
            entity_set,
            epochs,
            state,
            extensibility: Extensible,
        }
    }
}

impl<'a, Q, F, B> ViewValue<'a, Q, F, B, Extensible>
where
    Q: TupleQuery,
    F: Query,
    B: BorrowState,
{
    /// Extends query tuple with an additional query element.
    #[inline]
    pub fn extend<E>(self, ext: E) -> ViewValue<'a, TupleQueryAdd<Q, E>, F, B, Extensible>
    where
        E: SendQuery,
    {
        self.release_borrow();
        Q::extend_query(self.query, ext);

        let query = Q::extend_query(self.query, ext);
        let filter = self.filter;
        let archetypes = self.archetypes;
        let entity_set = self.entity_set;
        let epochs = self.epochs;
        let (state, Extensible) = self.extract();

        ViewValue {
            query,
            filter,
            archetypes,
            entity_set,
            epochs,
            state,
            extensibility: Extensible,
        }
    }

    /// Extends query tuple with a query element that fetches the component,
    /// filtering entities with the component and it was modified after the `after_epoch`.
    #[inline]
    pub fn modified<T>(
        self,
        after_epoch: EpochId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Modified<&'a T>>, F, B, Extensible>
    where
        T: Sync + 'static,
    {
        self.extend(Modified::<Read<T>>::new(after_epoch))
    }

    /// Extends query tuple with a query element that fetches the component,
    /// filtering entities with the component and it was modified after the `after_epoch`.
    #[inline]
    pub fn modified_mut<T>(
        self,
        after_epoch: EpochId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Modified<&'a mut T>>, F, B, Extensible>
    where
        T: Send + 'static,
    {
        self.extend(Modified::<Write<T>>::new(after_epoch))
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// First component of entity that provide `T` borrowing is used.
    /// If no component provides `T` borrowing, the entity is filtered out.
    #[inline]
    pub fn borrow_any<T>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, BorrowAny<&'a T>>, F, B, Extensible>
    where
        T: Sync + ?Sized + 'static,
    {
        self.extend(BorrowAny(Read::<T>))
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// First component of entity that provide `T` borrowing is used.
    /// If no component provides `T` borrowing, the entity is filtered out.
    #[inline]
    pub fn borrow_any_mut<T>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, BorrowAny<&'a mut T>>, F, B, Extensible>
    where
        T: Send + ?Sized + 'static,
    {
        self.extend(BorrowAny(Write::<T>))
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
    pub fn borrow_one<T>(
        self,
        ty: TypeId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, BorrowOne<&'a T>>, F, B, Extensible>
    where
        T: Sync + ?Sized + 'static,
    {
        self.extend(BorrowOne::<Read<T>>::new(ty))
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
    pub fn borrow_one_mut<T>(
        self,
        ty: TypeId,
    ) -> ViewValue<'a, TupleQueryAdd<Q, BorrowOne<&'a mut T>>, F, B, Extensible>
    where
        T: Send + ?Sized + 'static,
    {
        self.extend(BorrowOne::<Write<T>>::new(ty))
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
    pub fn borrow_all<T>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, BorrowAll<&'a T>>, F, B, Extensible>
    where
        T: Sync + ?Sized + 'static,
    {
        self.extend(BorrowAll(Read::<T>))
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain shared reference of the relation value
    /// and the target entity.
    #[inline]
    pub fn relates<R: Relation>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Relates<With<R>>>, F, B, Extensible> {
        self.extend(Relates::<With<R>>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain shared reference of the relation value
    /// and the target entity.
    #[inline]
    pub fn relates_ref<R: Relation + Sync>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Relates<&'a R>>, F, B, Extensible> {
        self.extend(Relates::<Read<R>>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain mutable reference of the relation value
    /// and the target entity.
    #[inline]
    pub fn relates_mut<R: Relation + Send>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Relates<&'a mut R>>, F, B, Extensible> {
        self.extend(Relates::<Write<R>>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain shared reference of the relation value
    /// and the target entity.
    #[inline]
    pub fn relates_to<R: Relation + Sync>(
        self,
        target: impl Entity,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesTo<&'a R>>, F, B, Extensible> {
        self.extend(RelatesTo::<Read<R>>::new(target.id()))
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain mutable reference of the relation value
    /// and the target entity.
    #[inline]
    pub fn relates_to_mut<R: Relation + Send>(
        self,
        target: impl Entity,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesTo<&'a mut R>>, F, B, Extensible> {
        self.extend(RelatesTo::<Write<R>>::new(target.id()))
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain shared reference of the relation value
    /// and the target entity.
    #[inline]
    pub fn relates_exclusive<R: ExclusiveRelation>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesExclusive<With<R>>>, F, B, Extensible> {
        self.extend(RelatesExclusive::<With<R>>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain shared reference of the relation value
    /// and the target entity.
    #[inline]
    pub fn relates_exclusive_ref<R: ExclusiveRelation + Sync>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesExclusive<&'a R>>, F, B, Extensible> {
        self.extend(RelatesExclusive::<Read<R>>)
    }

    /// Queries for origin entities in relation of type `R`.
    /// The view will contain mutable reference of the relation value
    /// and the target entity.
    #[inline]
    pub fn relates_exclusive_mut<R: ExclusiveRelation + Send>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, RelatesExclusive<&'a mut R>>, F, B, Extensible> {
        self.extend(RelatesExclusive::<Write<R>>)
    }

    /// Queries for target entities in relation of type `R`.
    /// The view will contain origins of the relation.
    #[inline]
    pub fn related<R: Relation>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Related<With<R>>>, F, B, Extensible> {
        self.extend(Related::<With<R>>)
    }

    /// Queries for target entities in relation of type `R`.
    /// The view will contain origins of the relation.
    #[inline]
    pub fn related_ref<R: Relation + Sync>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Related<Read<R>>>, F, B, Extensible> {
        self.extend(Related::<Read<R>>)
    }

    /// Queries for target entities in relation of type `R`.
    /// The view will contain origins of the relation.
    #[inline]
    pub fn related_mut<R: Relation + Send>(
        self,
    ) -> ViewValue<'a, TupleQueryAdd<Q, Related<Write<R>>>, F, B, Extensible> {
        self.extend(Related::<Write<R>>)
    }
}

impl<'a, Q, F, B> ViewValue<'a, Q, F, B, Extensible>
where
    Q: Query,
    F: TupleQuery,
    B: BorrowState,
{
    /// Extends filter tuple with an additional filter element.
    #[inline]
    pub fn filter<E>(self, ext: E) -> ViewValue<'a, Q, TupleQueryAdd<F, E>, B, Extensible>
    where
        E: SendQuery,
    {
        let query = self.query;
        let filter = F::extend_query(self.filter, ext);
        let archetypes = self.archetypes;
        let entity_set = self.entity_set;
        let epochs = self.epochs;
        let (state, Extensible) = self.extract();

        ViewValue {
            query,
            filter,
            archetypes,
            entity_set,
            epochs,
            state,
            extensibility: Extensible,
        }
    }

    /// Extends filter tuple with a filter element that
    /// filters entities that have the component.
    #[inline]
    pub fn with<T>(self) -> ViewValue<'a, Q, TupleQueryAdd<F, With<T>>, B, Extensible>
    where
        T: 'static,
    {
        self.filter(With)
    }

    /// Extends filter tuple with a filter element that\
    /// filters entities that do not have the component.
    #[inline]
    pub fn without<T>(self) -> ViewValue<'a, Q, TupleQueryAdd<F, Without<T>>, B, Extensible>
    where
        T: 'static,
    {
        self.filter(Not(With))
    }

    /// Extends filter tuple with a filter element that
    /// filters entities that have the component and it was modified after the `after_epoch`.
    #[inline]
    pub fn filter_modified<T>(
        self,
        after_epoch: EpochId,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, Modified<With<T>>>, B, Extensible>
    where
        T: 'static,
    {
        self.filter(Modified::<With<T>>::new(after_epoch))
    }

    /// Filters target entities in relation of type `R`.
    #[inline]
    pub fn filter_related<R: Relation>(
        self,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, FilterRelated<R>>, B, Extensible> {
        self.filter(FilterRelated)
    }

    /// Filters target entities in relation of type `R`
    /// with specified origin entity.
    #[inline]
    pub fn filter_related_by<R: Relation>(
        self,
        origin: impl Entity,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, FilterRelatedBy<R>>, B, Extensible> {
        self.filter(FilterRelatedBy::new(origin.id()))
    }

    /// Filters origin entities in relation of type `R`.
    #[inline]
    pub fn filter_relates<R: Relation>(
        self,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, FilterRelates<R>>, B, Extensible> {
        self.filter(FilterRelates)
    }

    /// Filters origin entities in relation of type `R`
    /// with specified target entity.
    #[inline]
    pub fn filter_relates_to<R: Relation>(
        self,
        target: impl Entity,
    ) -> ViewValue<'a, Q, TupleQueryAdd<F, FilterRelatesTo<R>>, B, Extensible> {
        self.filter(FilterRelatesTo::new(target.id()))
    }
}
