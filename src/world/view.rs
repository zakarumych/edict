use crate::{
    query::{DefaultQuery, DefaultSendQuery, IntoQuery, IntoSendQuery},
    view::{ViewCell, ViewMut, ViewValue},
};

use super::{World, WorldLocal};

impl World {
    /// Starts building new view.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    /// Use [`View`]'s methods to add sub-queries and filters.
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
    /// Use [`View`]'s methods to add sub-queries and filters.
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
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view<'a, Q>(&'a self) -> ViewCell<'a, (Q,)>
    where
        Q: DefaultSendQuery,
    {
        ViewValue::new_cell(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_mut<'a, Q>(&'a mut self) -> ViewMut<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        ViewValue::new(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
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

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_with<'a, Q>(&'a self, query: Q) -> ViewCell<'a, (Q,)>
    where
        Q: IntoSendQuery,
    {
        ViewValue::new_cell(self, (query.into_query(),), ())
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_with_mut<'a, Q>(&'a mut self, query: Q) -> ViewMut<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewValue::new(self, (query.into_query(),), ())
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
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
}

impl WorldLocal {
    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view<'a, Q>(&'a self) -> ViewCell<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        ViewValue::new_cell(self, (Q::default_query(),), ())
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view_with<'a, Q>(&'a self, query: Q) -> ViewCell<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewValue::new_cell(self, (query.into_query(),), ())
    }
}
