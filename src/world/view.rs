use crate::{
    query::{DefaultQuery, DefaultSendQuery, IntoQuery, IntoSendQuery},
    view::{Extensible, StaticallyBorrowed, ViewMut, ViewRef, ViewValue},
};

use super::{World, WorldLocal};

impl World {
    /// Starts building new view.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn new_view<'a>(&'a self) -> ViewRef<'a, ()> {
        ViewValue::new_ref(self, (), ())
    }

    /// Starts building new view.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn new_view_mut<'a>(&'a mut self) -> ViewMut<'a, ()> {
        ViewValue::new_mut(self, (), ())
    }

    /// Starts building new view.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    ///
    /// # Safety
    ///
    /// The caller is responsible that query won't create
    /// invalid aliasing of world's components.
    #[inline]
    pub unsafe fn new_view_unchecked<'a>(&'a mut self) -> ViewMut<'a, ()> {
        unsafe { ViewValue::new_unchecked(self, (), (), StaticallyBorrowed, Extensible) }
    }

    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view<'a, Q>(&'a self) -> ViewRef<'a, (Q,)>
    where
        Q: DefaultSendQuery,
    {
        ViewValue::new_ref(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// It requires default-constructible query and filter types.
    ///
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_filter<'a, Q, F>(&'a self) -> ViewRef<'a, (Q,), F>
    where
        Q: DefaultSendQuery,
        F: DefaultSendQuery,
    {
        ViewValue::new_ref(self, (Q::default_query(),), F::default_query())
    }

    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_mut<'a, Q>(&'a mut self) -> ViewMut<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        ViewValue::new_mut(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// It requires default-constructible query and filter types.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_filter_mut<'a, Q, F>(&'a mut self) -> ViewMut<'a, (Q,), F>
    where
        Q: DefaultQuery,
        F: DefaultQuery,
    {
        ViewValue::new_mut(self, (Q::default_query(),), F::default_query())
    }

    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    ///
    /// # Safety
    ///
    /// The caller is responsible that query won't create
    /// invalid aliasing of world's components.
    #[inline]
    pub unsafe fn view_unchecked<'a, Q>(&'a self) -> ViewMut<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        unsafe {
            ViewValue::new_unchecked(
                self,
                (Q::default_query(),),
                (),
                StaticallyBorrowed,
                Extensible,
            )
        }
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// It requires default-constructible query and filter types.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    ///
    /// # Safety
    ///
    /// The caller is responsible that query won't create
    /// invalid aliasing of world's components.
    #[inline]
    pub unsafe fn view_filter_unchecked<'a, Q, F>(&'a self) -> ViewMut<'a, (Q,), F>
    where
        Q: DefaultQuery,
        F: DefaultQuery,
    {
        unsafe {
            ViewValue::new_unchecked(
                self,
                (Q::default_query(),),
                F::default_query(),
                StaticallyBorrowed,
                Extensible,
            )
        }
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_with<'a, Q>(&'a self, query: Q) -> ViewRef<'a, (Q,)>
    where
        Q: IntoSendQuery,
    {
        ViewValue::new_ref(self, (query.into_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// Uses provided query and filter instances to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_filter_with<'a, Q, F>(&'a self, query: Q, filter: F) -> ViewRef<'a, (Q,), F>
    where
        Q: IntoSendQuery,
        F: IntoSendQuery,
    {
        ViewValue::new_ref(self, (query.into_query(),), filter.into_query())
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_with_mut<'a, Q>(&'a mut self, query: Q) -> ViewMut<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewValue::new_mut(self, (query.into_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// Uses provided query and filter instances to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_filter_with_mut<'a, Q, F>(&'a mut self, query: Q, filter: F) -> ViewMut<'a, (Q,), F>
    where
        Q: IntoQuery,
        F: IntoQuery,
    {
        ViewValue::new_mut(self, (query.into_query(),), filter.into_query())
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    ///
    /// # Safety
    ///
    /// The caller is responsible that query won't create
    /// invalid aliasing of world's components.
    #[inline]
    pub unsafe fn view_with_unchecked<'a, Q>(&'a self, query: Q) -> ViewMut<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        unsafe {
            ViewValue::new_unchecked(
                self,
                (query.into_query(),),
                (),
                StaticallyBorrowed,
                Extensible,
            )
        }
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// Uses provided query and filter instances to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    ///
    /// # Safety
    ///
    /// The caller is responsible that query won't create
    /// invalid aliasing of world's components.
    #[inline]
    pub unsafe fn view_filter_with_unchecked<'a, Q, F>(
        &'a self,
        query: Q,
        filter: F,
    ) -> ViewMut<'a, (Q,), F>
    where
        Q: IntoQuery,
        F: IntoQuery,
    {
        unsafe {
            ViewValue::new_unchecked(
                self,
                (query.into_query(),),
                filter.into_query(),
                StaticallyBorrowed,
                Extensible,
            )
        }
    }
}

impl WorldLocal {
    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view<'a, Q>(&'a self) -> ViewRef<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        ViewValue::new_ref(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// It requires default-constructible query and filter types.
    ///
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_filter<'a, Q, F>(&'a self) -> ViewRef<'a, (Q,), F>
    where
        Q: DefaultQuery,
        F: DefaultQuery,
    {
        ViewValue::new_ref(self, (Q::default_query(),), F::default_query())
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_with<'a, Q>(&'a self, query: Q) -> ViewRef<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewValue::new_ref(self, (query.into_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// Uses provided query and filter instances to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewRef`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_filter_with<'a, Q, F>(&'a self, query: Q, filter: F) -> ViewRef<'a, (Q,), F>
    where
        Q: IntoQuery,
        F: IntoQuery,
    {
        ViewValue::new_ref(self, (query.into_query(),), filter.into_query())
    }
}
