use core::{any::TypeId, marker::PhantomData};

/// A helper trait to extend tuples.
pub trait ExtendTuple<E>: Sized {
    /// Tuple with an additional element `E`.
    type Output;

    /// Extend tuple with an additional element `E`.
    fn extend_tuple(self, element: E) -> Self::Output;
}

/// A helper type alias to extend tuples.
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

impl<'a, Q, F, B> View<'a, Q, F, B>
where
    Q: IntoQuery,
    F: IntoQuery,
    B: BorrowState,
{
    /// Extends query tuple with an additional query element.
    #[inline]
    pub fn extend_query<E>(self, query: E) -> View<'a, TuplePlus<Q, E>, F, B>
    where
        Q::Query: ExtendTuple<E>,
        E: IntoQuery,
    {
        View {
            query: self.query.extend_tuple(query),
            filter: self.filter,
            archetypes: self.archetypes,
            entity_set: self.entity_set,
            borrow_state: self.borrow_state,
        }
    }

    /// Extends filter tuple with an additional filter element.
    #[inline]
    pub fn extend_filter<E>(self, filter: E) -> View<'a, Q, TuplePlus<F, E>, B>
    where
        F::Query: ExtendTuple<E>,
        E: IntoQuery,
    {
        View {
            query: self.query,
            filter: self.filter.extend_tuple(filter),
            archetypes: self.archetypes,
            entity_set: self.entity_set,
            borrow_state: self.borrow_state,
        }
    }

    /// Extends filter tuple with a filter element that
    /// filters entities that have the component.
    #[inline]
    pub fn with<T>(self) -> View<'a, Q, TuplePlus<F, With<T>>, B>
    where
        F::Query: ExtendTuple<With<T>>,
    {
        self.extend_filter(PhantomData)
    }

    /// Extends filter tuple with a filter element that\
    /// filters entities that do not have the component.
    #[inline]
    pub fn without<T>(self) -> View<'a, Q, TuplePlus<F, Without<T>>, B>
    where
        F::Query: ExtendTuple<Without<T>>,
    {
        self.extend_filter(PhantomData)
    }

    /// Extends query tuple with a query element that fetches the component,
    /// filtering entities with the component and it was modified after the `after_epoch`.
    #[inline]
    pub fn modified<T>(self, after_epoch: EpochId) -> View<'a, TuplePlus<Q, Modified<T>>, F, B>
    where
        Q::Query: ExtendTuple<Modified<T>>,
    {
        self.extend_query(Modified::new(after_epoch))
    }

    /// Extends filter tuple with a filter element that
    /// filters entities that have the component and it was modified after the `after_epoch`.
    #[inline]
    pub fn filter_modified<T>(self, after_epoch: EpochId) -> View<'a, Q, TuplePlus<F, Modified<With<T>>>, B>
    where
        F::Query: ExtendTuple<Modified<With<T>>>,
    {
        self.extend_filter(Modified::new(after_epoch))
    }

    /// Extends query tuple with a query element that fetches borrows `T`
    /// from a component of the entity.
    /// First component of entity that provide `T` borrowing is used.
    /// If no component provides `T` borrowing, the entity is filtered out. 
    #[inline]
    pub fn borrow_any<T>(self) -> View<'a, TuplePlus<Q, QueryBorrowAny<T>>, F, B> {
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
    pub fn borrow_one<T>(self, id: TypeId) -> View<'a, TuplePlus<Q, QueryBorrowOne<T>>, F, B> {
        self.extend_query(QueryBorrowOne::new(id))
    }

    /// Extends query tuple with a query element that fetches relation.
    /// Filters entities that are origin of the relation `R`.
    /// Fetches relation value reference `&R`.
    #[inline]
    pub fn relates<R>(self) -> View<'a, TuplePlus<Q, Relates<R>>, F, B> {
        self.extend_query(PhantomData)
    }
}
