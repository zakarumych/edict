use core::any::TypeId;

use crate::{archetype::Archetype, component::Component};

use super::{fetch::UnitFetch, merge_access, Access, Fetch, ImmutableQuery, Query};

/// Tuple of null items.
pub trait FilterItem {}

impl FilterItem for () {}
impl<A, B> FilterItem for (A, B)
where
    A: FilterItem,
    B: FilterItem,
{
}

pub trait FilterFetch<'a>: Fetch<'a, Item = Self::FilterItem> {
    type FilterItem: FilterItem;
}

impl<'a, F> FilterFetch<'a> for F
where
    F: Fetch<'a>,
    F::Item: FilterItem,
{
    type FilterItem = F::Item;
}

/// Filters are queries that yield nothing.
/// Filters are automatically implemented for `ImmutableQuery` implementations where `Item = ()`.
/// This means that user cannot implement `Filter` manually and should implement `Query` instead.
pub trait Filter: ImmutableQuery<Fetch = Self::FilterFetch> {
    /// Fetch type that yields nothing.
    /// Used to ensure `Fetch` also yields nothing in the bounds above.
    type FilterFetch: for<'a> FilterFetch<'a>;
}

impl<Q> Filter for Q
where
    Q: ImmutableQuery,
    Q::Fetch: for<'a> FilterFetch<'a>,
{
    type FilterFetch = Q::Fetch;
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
    F: FilterFetch<'a>,
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
    pub(super) filter: F,
    pub(super) query: Q,
}

unsafe impl<F, Q> Query for FilteredQuery<F, Q>
where
    F: Filter,
    Q: Query,
{
    type Fetch = FilteredFetch<F::Fetch, Q::Fetch>;

    #[inline]
    fn is_valid(&self) -> bool {
        self.query.is_valid()
    }

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        merge_access(self.filter.access(ty), self.query.access(ty))
    }

    #[inline]
    fn conflicts<U>(&self, query: &U) -> bool
    where
        U: Query,
    {
        self.filter.conflicts(query) || self.query.conflicts(query)
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        self.filter.skip_archetype(archetype) || self.query.skip_archetype(archetype)
    }

    #[inline]
    unsafe fn fetch(
        &mut self,
        archetype: &Archetype,
        index: u64,
    ) -> FilteredFetch<F::Fetch, Q::Fetch> {
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

unsafe impl<T> Query for With<T>
where
    T: Component,
{
    type Fetch = UnitFetch;

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn conflicts<U>(&self, _: &U) -> bool
    where
        U: Query,
    {
        false
    }

    #[inline]
    fn access(&self, _: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(&mut self, _: &Archetype, _: u64) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutableQuery for With<T> where T: Component {}

phantom_newtype! {
    /// Filter that allows only archetypes without specified component.
    pub struct Without<T>
}

unsafe impl<T> Query for Without<T>
where
    T: Component,
{
    type Fetch = UnitFetch;

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn conflicts<U>(&self, _: &U) -> bool
    where
        U: Query,
    {
        false
    }

    #[inline]
    fn access(&self, _: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(&mut self, _: &Archetype, _: u64) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutableQuery for Without<T> where T: Component {}
