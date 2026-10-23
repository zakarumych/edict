use crate::{
    query::{AsDefaultQuery, AsDefaultSendQuery, IntoQuery, IntoSendQuery},
    view::{
        Extensible, LocalViewMut, LocalViewRef, RuntimeBorrowState, StaticallyBorrowed, ViewMut,
        ViewRef, ViewValue,
    },
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
        Q: AsDefaultSendQuery,
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
        Q: AsDefaultSendQuery,
        F: AsDefaultSendQuery,
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
        Q: AsDefaultQuery,
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
        Q: AsDefaultQuery,
        F: AsDefaultQuery,
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
        Q: AsDefaultQuery,
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
        Q: AsDefaultQuery,
        F: AsDefaultQuery,
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
    /// Starts a local view with no subqueries.
    #[inline]
    pub fn new_view(&self) -> LocalViewRef<'_, ()> {
        // Safety: this local borrow excludes shared-world epoch access.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (),
                (),
                RuntimeBorrowState::new(),
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Starts an exclusively borrowed local view with no subqueries.
    #[inline]
    pub fn new_view_mut(&mut self) -> LocalViewMut<'_, ()> {
        // Safety: exclusive local access excludes component aliases and shared epochs.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (),
                (),
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Starts a statically borrowed local view.
    ///
    /// # Safety
    /// Extended queries must not create conflicting component references.
    #[inline]
    pub unsafe fn new_view_unchecked(&mut self) -> LocalViewMut<'_, ()> {
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (),
                (),
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates a local view with a default query and runtime component borrow checks.
    #[inline]
    pub fn view<Q: AsDefaultQuery>(&self) -> LocalViewRef<'_, (Q,)> {
        // Safety: local epochs cannot escape to shared-world access; components are checked.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (Q::default_query(),),
                (),
                RuntimeBorrowState::new(),
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates a local view with default query and filter.
    #[inline]
    pub fn view_filter<Q: AsDefaultQuery, F: AsDefaultQuery>(&self) -> LocalViewRef<'_, (Q,), F> {
        // Safety: local epochs cannot escape to shared-world access; components are checked.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (Q::default_query(),),
                F::default_query(),
                RuntimeBorrowState::new(),
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates an exclusively borrowed local view with a default query.
    #[inline]
    pub fn view_mut<Q: AsDefaultQuery>(&mut self) -> LocalViewMut<'_, (Q,)> {
        let query = (Q::default_query(),);
        for archetype in self.inner.archetypes() {
            crate::view::validate_query_filter(query, (), archetype);
        }
        // Safety: exclusive access excludes component aliases; the tuple is validated.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                query,
                (),
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates an exclusively borrowed local view with default query and filter.
    #[inline]
    pub fn view_filter_mut<Q: AsDefaultQuery, F: AsDefaultQuery>(
        &mut self,
    ) -> LocalViewMut<'_, (Q,), F> {
        let query = (Q::default_query(),);
        let filter = F::default_query();
        for archetype in self.inner.archetypes() {
            crate::view::validate_query_filter(query, filter, archetype);
        }
        // Safety: exclusive access excludes component aliases; query/filter are validated.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                query,
                filter,
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates a statically borrowed local view with a default query.
    ///
    /// # Safety
    /// The query must not conflict with any active component references.
    #[inline]
    pub unsafe fn view_unchecked<Q: AsDefaultQuery>(&self) -> LocalViewMut<'_, (Q,)> {
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (Q::default_query(),),
                (),
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates a statically borrowed local view with default query and filter.
    ///
    /// # Safety
    /// Query and filter must not create conflicting component references.
    #[inline]
    pub unsafe fn view_filter_unchecked<Q: AsDefaultQuery, F: AsDefaultQuery>(
        &self,
    ) -> LocalViewMut<'_, (Q,), F> {
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (Q::default_query(),),
                F::default_query(),
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates a local view with a provided query and runtime component borrow checks.
    #[inline]
    pub fn view_with<Q: IntoQuery>(&self, query: Q) -> LocalViewRef<'_, (Q,)> {
        // Safety: local epochs cannot escape to shared-world access; components are checked.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (query.into_query(),),
                (),
                RuntimeBorrowState::new(),
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates a local view with a provided query and filter.
    #[inline]
    pub fn view_filter_with<Q: IntoQuery, F: IntoQuery>(
        &self,
        query: Q,
        filter: F,
    ) -> LocalViewRef<'_, (Q,), F> {
        // Safety: local epochs cannot escape to shared-world access; components are checked.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (query.into_query(),),
                filter.into_query(),
                RuntimeBorrowState::new(),
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates an exclusively borrowed local view with a provided query.
    #[inline]
    pub fn view_with_mut<Q: IntoQuery>(&mut self, query: Q) -> LocalViewMut<'_, (Q,)> {
        let query = (query.into_query(),);
        for archetype in self.inner.archetypes() {
            crate::view::validate_query_filter(query, (), archetype);
        }
        // Safety: exclusive access excludes component aliases; the tuple is validated.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                query,
                (),
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates an exclusively borrowed local view with a provided query and filter.
    #[inline]
    pub fn view_filter_with_mut<Q: IntoQuery, F: IntoQuery>(
        &mut self,
        query: Q,
        filter: F,
    ) -> LocalViewMut<'_, (Q,), F> {
        let query = (query.into_query(),);
        let filter = filter.into_query();
        for archetype in self.inner.archetypes() {
            crate::view::validate_query_filter(query, filter, archetype);
        }
        // Safety: exclusive access excludes component aliases; query/filter are validated.
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                query,
                filter,
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates a statically borrowed local view with a provided query.
    ///
    /// # Safety
    /// The query must not conflict with any active component references.
    #[inline]
    pub unsafe fn view_with_unchecked<Q: IntoQuery>(&self, query: Q) -> LocalViewMut<'_, (Q,)> {
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (query.into_query(),),
                (),
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }

    /// Creates a statically borrowed local view with a provided query and filter.
    ///
    /// # Safety
    /// Query and filter must not create conflicting component references.
    #[inline]
    pub unsafe fn view_filter_with_unchecked<Q: IntoQuery, F: IntoQuery>(
        &self,
        query: Q,
        filter: F,
    ) -> LocalViewMut<'_, (Q,), F> {
        unsafe {
            ViewValue::new_with_epochs(
                &self.inner,
                (query.into_query(),),
                filter.into_query(),
                StaticallyBorrowed,
                Extensible,
                self.local_epochs(),
            )
        }
    }
}
