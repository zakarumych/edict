use crate::{
    query::{DefaultQuery, IntoQuery},
    view::{View, ViewMut, ViewState},
};

use super::World;

impl World {
    /// Starts building new view.
    ///
    /// Returned query matches all entities and yields `()` for every one of them.
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn new_view<'a>(&'a self) -> View<'a, ()> {
        ViewState::new(self)
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
        ViewState::new_mut(self)
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
        unsafe { ViewState::new_unchecked(self) }
    }

    /// Creates new view with single sub-query.
    ///
    /// It requires default-constructible query type.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline(always)]
    pub fn view<'a, Q>(&'a self) -> View<'a, (Q,)>
    where
        Q: DefaultQuery,
    {
        ViewState::new(self)
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
        ViewState::new_mut(self)
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
        unsafe { ViewState::new_unchecked(self) }
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_with<'a, Q>(&'a self, query: Q) -> View<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewState::with_query(self, query)
    }

    /// Creates new view with single sub-query.
    ///
    /// Uses provided query instance to support stateful queries.
    ///
    /// Borrows world mutably to avoid runtime borrow checks.
    ///
    /// Use [`View`]'s methods to add sub-queries and filters.
    #[inline]
    pub fn view_with_mut<'a, Q>(&'a mut self, query: Q) -> ViewMut<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        ViewState::with_query_mut(self, query)
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
    #[inline]
    pub unsafe fn view_with_unchecked<'a, Q>(&'a self, query: Q) -> ViewMut<'a, (Q,)>
    where
        Q: IntoQuery,
    {
        unsafe { ViewState::with_query_unchecked(self, query) }
    }
}
