use crate::{
    query::{DefaultQuery, DefaultSendQuery, IntoQuery, IntoSendQuery},
    view::{ViewCell, ViewMut, ViewValue},
};

use super::{World, WorldLocal};

impl World {
    /// Starts building new view.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn new_view<'a>(&'a self) -> ViewCell<'a, ()> {
        ViewValue::new_cell(self, (), ())
    }

    /// Starts building new view.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn new_view_mut<'a>(&'a mut self) -> ViewMut<'a, ()> {
        ViewValue::new(self, (), ())
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
    #[inline(always)]
    pub unsafe fn new_view_unchecked<'a>(&'a mut self) -> ViewMut<'a, ()> {
        unsafe { ViewValue::new_unchecked(self, (), ()) }
    }

    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view<'a, Q>(&'a self) -> ViewCell<'a, (Q,)>
    where
        Q: DefaultSendQuery,
    {
        ViewValue::new_cell(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// It requires default-constructible query and filter types.
    ///
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_filter<'a, Q, F>(&'a self) -> ViewCell<'a, (Q,), F>
    where
        Q: DefaultSendQuery,
        F: DefaultSendQuery,
    {
        ViewValue::new_cell(self, (Q::default_query(),), F::default_query())
    }

    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_mut<'a, Q>(&'a mut self) -> ViewMut<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        ViewValue::new(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// It requires default-constructible query and filter types.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_filter_mut<'a, Q, F>(&'a mut self) -> ViewMut<'a, (Q,), F>
    where
        Q: DefaultQuery,
        F: DefaultQuery,
    {
        ViewValue::new(self, (Q::default_query(),), F::default_query())
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
    #[inline(always)]
    pub unsafe fn view_unchecked<'a, Q>(&'a self) -> ViewMut<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        unsafe { ViewValue::new_unchecked(self, (Q::default_query(),), ()) }
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
    #[inline(always)]
    pub unsafe fn view_filter_unchecked<'a, Q, F>(&'a self) -> ViewMut<'a, (Q,), F>
    where
        Q: DefaultQuery,
        F: DefaultQuery,
    {
        unsafe { ViewValue::new_unchecked(self, (Q::default_query(),), F::default_query()) }
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_with<'a, Q>(&'a self, query: Q) -> ViewCell<'a, (Q,)>
    where
        Q: IntoSendQuery,
    {
        ViewValue::new_cell(self, (query.into_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// Uses provided query and filter instances to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_filter_with<'a, Q, F>(&'a self, query: Q, filter: F) -> ViewCell<'a, (Q,), F>
    where
        Q: IntoSendQuery,
        F: IntoSendQuery,
    {
        ViewValue::new_cell(self, (query.into_query(),), filter.into_query())
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_with_mut<'a, Q>(&'a mut self, query: Q) -> ViewMut<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewValue::new(self, (query.into_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// Uses provided query and filter instances to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewMut`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_filter_with_mut<'a, Q, F>(&'a mut self, query: Q, filter: F) -> ViewMut<'a, (Q,), F>
    where
        Q: IntoQuery,
        F: IntoQuery,
    {
        ViewValue::new(self, (query.into_query(),), filter.into_query())
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
    #[inline(always)]
    pub unsafe fn view_with_unchecked<'a, Q>(&'a self, query: Q) -> ViewMut<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        unsafe { ViewValue::new_unchecked(self, (query.into_query(),), ()) }
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
    #[inline(always)]
    pub unsafe fn view_filter_with_unchecked<'a, Q, F>(
        &'a self,
        query: Q,
        filter: F,
    ) -> ViewMut<'a, (Q,), F>
    where
        Q: IntoQuery,
        F: IntoQuery,
    {
        unsafe { ViewValue::new_unchecked(self, (query.into_query(),), filter.into_query()) }
    }
}

impl WorldLocal {
    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view<'a, Q>(&'a self) -> ViewCell<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        ViewValue::new_cell(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// It requires default-constructible query and filter types.
    ///
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_filter<'a, Q, F>(&'a self) -> ViewCell<'a, (Q,), F>
    where
        Q: DefaultQuery,
        F: DefaultQuery,
    {
        ViewValue::new_cell(self, (Q::default_query(),), F::default_query())
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_with<'a, Q>(&'a self, query: Q) -> ViewCell<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewValue::new_cell(self, (query.into_query(),), ())
    }

    /// Creates new view with single sub-query and filter.
    ///
    /// Uses provided query and filter instances to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`ViewCell`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_filter_with<'a, Q, F>(&'a self, query: Q, filter: F) -> ViewCell<'a, (Q,), F>
    where
        Q: IntoQuery,
        F: IntoQuery,
    {
        ViewValue::new_cell(self, (query.into_query(),), filter.into_query())
    }
}
