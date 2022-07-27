use core::{any::TypeId, marker::PhantomData};

use crate::{archetype::Archetype, component::Component};

use super::{fetch::NullFetch, merge_access, Access, Fetch, ImmutableQuery, Query};

/// Filters are queries that yield nothing.
/// Filters are automatically implemented for `ImmutableQuery` implementations where `Item = ()`.
/// This means that user cannot implement `Filter` manually and should implement `Query` instead.
pub unsafe trait Filter: ImmutableQuery<Fetch = Self::NullFetch> {
    /// Fetch type that yields nothing.
    /// Used to ensure `Fetch` also yields nothing in the bounds above.
    type NullFetch: for<'a> Fetch<'a, Item = ()>;
}

unsafe impl<Q> Filter for Q
where
    Q: ImmutableQuery,
    Q::Fetch: for<'a> Fetch<'a, Item = ()>,
{
    type NullFetch = Q::Fetch;
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
    F: Fetch<'a, Item = ()>,
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

/// Filter that allows only archetypes with specified component.
#[derive(Clone, Copy, Debug, Default)]
pub struct With<T> {
    marker: PhantomData<T>,
}

impl<T> With<T> {
    /// Returns new instance of `With` filter.
    pub const fn new() -> Self {
        With {
            marker: PhantomData,
        }
    }
}

unsafe impl<T> Query for With<T>
where
    T: Component,
{
    type Fetch = NullFetch;

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
    unsafe fn fetch(&mut self, _: &Archetype, _: u64) -> NullFetch {
        NullFetch::new()
    }
}

/// Filter that allows only archetypes without specified component.
#[derive(Clone, Copy, Debug, Default)]
pub struct Without<T> {
    marker: PhantomData<T>,
}

impl<T> Without<T> {
    /// Returns new instance of `Without` filter.
    pub const fn new() -> Self {
        Without {
            marker: PhantomData,
        }
    }
}

unsafe impl<T> Query for Without<T>
where
    T: Component,
{
    type Fetch = NullFetch;

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
    unsafe fn fetch(&mut self, _: &Archetype, _: u64) -> NullFetch {
        NullFetch::new()
    }
}
