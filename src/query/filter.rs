use core::{any::TypeId, marker::PhantomData};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{
    fetch::UnitFetch, merge_access, Access, Fetch, ImmutablePhantomQuery, ImmutableQuery,
    IntoQuery, PhantomQuery, PhantomQueryFetch, Query, QueryFetch,
};

/// Tuple of filter items.
pub trait FilterItem: 'static {}

impl FilterItem for () {}
impl<A, B> FilterItem for (A, B)
where
    A: FilterItem,
    B: FilterItem,
{
}

pub trait FilterFetch<'a>: QueryFetch<'a, Item = Self::FilterItem> {
    type FilterItem: FilterItem;
}

/// Filters are queries that yield nothing.
/// Filters are automatically implemented for `ImmutableQuery` implementations where `Item = ()`.
/// This means that user cannot implement `Filter` manually and should implement `Query` instead.
pub trait Filter:
    for<'a> FilterFetch<'a>
    + ImmutableQuery
    + for<'a> QueryFetch<'a, Item = <Self as FilterFetch<'a>>::FilterItem>
{
}

/// Types associated with a filter type.
pub trait IntoFilter: IntoQuery<Query = Self::Filter> {
    /// Associated filter type.
    type Filter: Filter;
}

impl<T> IntoFilter for T
where
    T: IntoQuery,
    T::Query: Filter,
{
    type Filter = T::Query;
}

impl<'a, Q> FilterFetch<'a> for Q
where
    Q: ImmutableQuery,
    <Q as QueryFetch<'a>>::Item: FilterItem,
{
    type FilterItem = <Q as QueryFetch<'a>>::Item;
}

impl<Q> Filter for Q
where
    Q: ImmutableQuery,
    for<'a> <Q as QueryFetch<'a>>::Item: FilterItem,
{
}

/// Combines fetch from query and filter.
/// Skips using both and yields using query.
#[derive(Clone, Copy, Debug)]
pub struct FilteredFetch<F, Q> {
    filter: F,
    query: Q,
}

unsafe impl<'a, F, Q> Fetch<'a> for FilteredFetch<F, Q>
where
    F: Fetch<'a>,
    Q: Fetch<'a>,
{
    type Item = Q::Item;

    #[inline]
    fn dangling() -> Self {
        FilteredFetch {
            filter: F::dangling(),
            query: Q::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        self.filter.skip_chunk(chunk_idx) || self.query.skip_chunk(chunk_idx)
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        self.filter.visit_chunk(chunk_idx);
        self.query.visit_chunk(chunk_idx);
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        self.filter.skip_item(idx) || self.query.skip_item(idx)
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> Self::Item {
        self.filter.get_item(idx);
        self.query.get_item(idx)
    }
}

/// Combines query and filter.
/// Skips using both and yields using query.
#[derive(Clone, Copy, Debug)]
pub struct FilteredQuery<F, Q> {
    pub(crate) filter: F,
    pub(crate) query: Q,
}

impl<'a, F, Q> QueryFetch<'a> for FilteredQuery<F, Q>
where
    F: Filter,
    Q: Query,
{
    type Item = <Q as QueryFetch<'a>>::Item;
    type Fetch = FilteredFetch<<F as QueryFetch<'a>>::Fetch, <Q as QueryFetch<'a>>::Fetch>;
}

impl<F, Q> IntoQuery for FilteredQuery<F, Q>
where
    F: Filter,
    Q: Query,
{
    type Query = FilteredQuery<F, Q>;
}

impl<F, Q> Query for FilteredQuery<F, Q>
where
    F: Filter,
    Q: Query,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        merge_access(self.filter.access(ty), self.query.access(ty))
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        self.filter.skip_archetype(archetype) || self.query.skip_archetype(archetype)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        index: EpochId,
    ) -> FilteredFetch<<F as QueryFetch<'a>>::Fetch, <Q as QueryFetch<'a>>::Fetch> {
        FilteredFetch {
            filter: self.filter.fetch(archetype, index),
            query: self.query.fetch(archetype, index),
        }
    }
}

unsafe impl<F, Q> ImmutableQuery for FilteredQuery<F, Q>
where
    Q: ImmutableQuery,
    F: Filter,
{
}

phantom_newtype! {
    /// Filter that allows only archetypes with specified component.
    pub struct With<T>
}

impl<T> IntoQuery for With<T>
where
    T: 'static,
{
    type Query = PhantomData<fn() -> With<T>>;
}

impl<T> PhantomQueryFetch<'_> for With<T>
where
    T: 'static,
{
    type Item = ();
    type Fetch = UnitFetch;
}

impl<T> PhantomQuery for With<T>
where
    T: 'static,
{
    #[inline]
    fn access(_: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(_: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutablePhantomQuery for With<T> where T: 'static {}

phantom_newtype! {
    /// Filter that allows only archetypes without specified component.
    pub struct Without<T>
}

impl<T> IntoQuery for Without<T>
where
    T: 'static,
{
    type Query = PhantomData<fn() -> Without<T>>;
}

impl<T> PhantomQueryFetch<'_> for Without<T>
where
    T: 'static,
{
    type Item = ();
    type Fetch = UnitFetch;
}

impl<T> PhantomQuery for Without<T>
where
    T: 'static,
{
    #[inline]
    fn access(_: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(_: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutablePhantomQuery for Without<T> where T: 'static {}
