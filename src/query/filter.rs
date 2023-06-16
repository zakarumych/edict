use core::{
    any::{type_name, TypeId},
    marker::PhantomData,
};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{
    fetch::UnitFetch, try_merge_access, Access, Fetch, ImmutablePhantomQuery, ImmutableQuery,
    IntoQuery, PhantomQuery, Query,
};

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

    #[inline(always)]
    fn dangling() -> Self {
        FilteredFetch {
            filter: F::dangling(),
            query: Q::dangling(),
        }
    }

    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        self.filter.visit_chunk(chunk_idx) && self.query.visit_chunk(chunk_idx)
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        self.filter.touch_chunk(chunk_idx);
        self.query.touch_chunk(chunk_idx);
    }

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        self.filter.visit_item(idx) && self.query.visit_item(idx)
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> Self::Item {
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

impl<F, Q> IntoQuery for FilteredQuery<F, Q>
where
    F: IntoQuery,
    Q: IntoQuery,
{
    type Query = FilteredQuery<F::Query, Q::Query>;

    #[inline(always)]
    fn into_query(self) -> Self::Query {
        FilteredQuery {
            filter: self.filter.into_query(),
            query: self.query.into_query(),
        }
    }
}

unsafe impl<F, Q> Query for FilteredQuery<F, Q>
where
    F: Query,
    Q: Query,
{
    type Item<'a> = Q::Item<'a>;
    type Fetch<'a> = FilteredFetch<F::Fetch<'a>, Q::Fetch<'a>>;

    const MUTABLE: bool = F::MUTABLE || Q::MUTABLE;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        match try_merge_access(self.filter.access(ty), self.query.access(ty)) {
            Ok(access) => access,
            Err(_) => panic!(
                "Query '{}' and filter '{}' access conflict",
                type_name::<Q>(),
                type_name::<F>(),
            ),
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.filter.visit_archetype(archetype) && self.query.visit_archetype(archetype)
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        index: EpochId,
    ) -> FilteredFetch<F::Fetch<'a>, Q::Fetch<'a>> {
        FilteredFetch {
            filter: self.filter.fetch(arch_idx, archetype, index),
            query: self.query.fetch(arch_idx, archetype, index),
        }
    }
}

unsafe impl<F, Q> ImmutableQuery for FilteredQuery<F, Q>
where
    Q: ImmutableQuery,
    F: ImmutableQuery,
{
}

/// Inverse of a filter.
/// Entities that match the filter are skipped.
///
/// The `Not` filter will NOT cause side effects of the inner filter.
#[derive(Clone, Copy)]
pub struct Not<T>(pub T);

pub struct NotFetch<T>(T, bool);

unsafe impl<'a, T> Fetch<'a> for NotFetch<T>
where
    T: Fetch<'a>,
{
    type Item = ();

    #[inline(always)]
    fn dangling() -> Self {
        NotFetch(T::dangling(), false)
    }

    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        self.1 = self.0.visit_chunk(chunk_idx);
        true
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, _chunk_idx: u32) {}

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        if self.1 {
            !self.0.visit_item(idx)
        } else {
            true
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, _idx: u32) {}
}

impl<T> IntoQuery for Not<T>
where
    T: IntoQuery,
{
    type Query = Not<T::Query>;

    #[inline(always)]
    fn into_query(self) -> Self::Query {
        Not(self.0.into_query())
    }
}

unsafe impl<T> Query for Not<T>
where
    T: Query,
{
    type Item<'a> = ();
    type Fetch<'a> = NotFetch<T::Fetch<'a>>;

    const MUTABLE: bool = T::MUTABLE;

    #[inline(always)]
    fn access(&self, _: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        !self.0.visit_archetype(archetype)
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> NotFetch<T::Fetch<'a>> {
        NotFetch(self.0.fetch(arch_idx, archetype, epoch), false)
    }
}

phantom_newtype! {
    /// [`Filter`] that allows only archetypes with specified component.
    pub struct With<T>
}

impl<T> With<T>
where
    T: 'static,
{
    /// Creates a new [`Entities`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

unsafe impl<T> PhantomQuery for With<T>
where
    T: 'static,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    const MUTABLE: bool = false;

    #[inline(always)]
    fn access(_: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(_archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline(always)]
    unsafe fn fetch(_: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutablePhantomQuery for With<T> where T: 'static {}

/// [`Filter`] that allows only archetypes without specified component.
/// Inverse of [`With`].
pub type Without<T> = Not<With<T>>;
