use core::{any::TypeId, marker::PhantomData};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{
    fetch::UnitFetch, merge_access, Access, Fetch, ImmutablePhantomQuery, ImmutableQuery,
    IntoQuery, PhantomQuery, Query,
};

// /// Tuple of filter items.
// pub trait FilterItem: 'static {}

// impl FilterItem for () {}
// impl<A, B> FilterItem for (A, B)
// where
//     A: FilterItem,
//     B: FilterItem,
// {
// }

// pub trait FilterHelper<'a>: Query<Item<'a> = Self::FilterItemHelper> {
//     type FilterItemHelper: FilterItem;
// }

// impl<'a, F> FilterHelper<'a> for F
// where
//     F: Filter,
// {
//     type FilterItemHelper = F::FilterItem<'a>;
// }

// /// Filters are queries that yield nothing.
// /// Filters are automatically implemented for `ImmutableQuery` implementations where `Item = ()`.
// /// This means that user cannot implement `Filter` manually and should implement `Query` instead.
// pub trait Filter: for<'a> FilterHelper<'a, FilterItemHelper = Self::FilterItem<'a>> {
//     type FilterItem<'a>: FilterItem;
// }

// /// Types associated with a filter type.
// pub trait IntoFilter: IntoQuery<Query = Self::Filter> {
//     /// Associated filter type.
//     type Filter: Filter;
// }

// impl<T> IntoFilter for T
// where
//     T: IntoQuery,
//     T::Query: Filter,
// {
//     type Filter = T::Query;
// }

// impl<Q> Filter for Q
// where
//     Q: ImmutableQuery,
//     for<'a> Q::Item<'a>: FilterItem,
// {
// }

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
    F: Query,
    Q: Query,
{
    type Query = FilteredQuery<F, Q>;
}

unsafe impl<F, Q> Query for FilteredQuery<F, Q>
where
    F: Query,
    Q: Query,
{
    type Item<'a> = Q::Item<'a>;
    type Fetch<'a> = FilteredFetch<F::Fetch<'a>, Q::Fetch<'a>>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        merge_access(self.filter.access(ty), self.query.access(ty))
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        self.filter.skip_archetype(archetype) || self.query.skip_archetype(archetype)
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        index: EpochId,
    ) -> FilteredFetch<F::Fetch<'a>, Q::Fetch<'a>> {
        FilteredFetch {
            filter: self.filter.fetch(archetype, index),
            query: self.query.fetch(archetype, index),
        }
    }
}

unsafe impl<F, Q> ImmutableQuery for FilteredQuery<F, Q>
where
    Q: ImmutableQuery,
    F: ImmutableQuery,
{
}

phantom_newtype! {
    /// [`Filter`] that allows only archetypes with specified component.
    pub struct With<T>
}

impl<T> IntoQuery for With<T>
where
    T: 'static,
{
    type Query = PhantomData<fn() -> With<T>>;
}

unsafe impl<T> PhantomQuery for With<T>
where
    T: 'static,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    #[inline]
    fn access(_: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.has_component(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(_: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutablePhantomQuery for With<T> where T: 'static {}

phantom_newtype! {
    /// [`Filter`] that allows only archetypes without specified component.
    pub struct Without<T>
}

impl<T> IntoQuery for Without<T>
where
    T: 'static,
{
    type Query = PhantomData<fn() -> Without<T>>;
}

unsafe impl<T> PhantomQuery for Without<T>
where
    T: 'static,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    #[inline]
    fn access(_: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(_: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutablePhantomQuery for Without<T> where T: 'static {}

/// Combines two filters and yields only entities that pass both.
pub struct And<A, B>(pub A, pub B);

pub struct AndFetch<A, B> {
    a: A,
    b: B,
}

unsafe impl<'a, A, B> Fetch<'a> for AndFetch<A, B>
where
    A: Fetch<'a>,
    B: Fetch<'a>,
{
    type Item = ();

    #[inline(always)]
    fn dangling() -> Self {
        Self {
            a: A::dangling(),
            b: B::dangling(),
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, _idx: usize) {}

    #[inline(always)]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        self.a.skip_item(idx) || self.b.skip_item(idx)
    }

    #[inline(always)]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        self.a.skip_chunk(chunk_idx) || self.b.skip_chunk(chunk_idx)
    }

    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        self.a.visit_chunk(chunk_idx);
        self.b.visit_chunk(chunk_idx);
    }
}

impl<A, B> IntoQuery for And<A, B>
where
    A: IntoQuery,
    B: IntoQuery,
{
    type Query = And<A::Query, B::Query>;
}

unsafe impl<A, B> Query for And<A, B>
where
    A: Query,
    B: Query,
{
    type Item<'a> = ();
    type Fetch<'a> = AndFetch<A::Fetch<'a>, B::Fetch<'a>>;

    fn access(&self, ty: TypeId) -> Option<Access> {
        merge_access(self.0.access(ty), self.1.access(ty))
    }

    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        self.0.access_archetype(archetype, f);
        self.1.access_archetype(archetype, f);
    }

    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        self.0.skip_archetype(archetype) || self.1.skip_archetype(archetype)
    }

    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> AndFetch<A::Fetch<'a>, B::Fetch<'a>> {
        AndFetch {
            a: self.0.fetch(archetype, epoch),
            b: self.1.fetch(archetype, epoch),
        }
    }
}

/// Combines two filters and yields entities that pass either.
pub struct Or<A, B>(pub A, pub B);

/// Combines two filters and yields entities that pass only one of them.
pub struct Xor<A, B>(pub A, pub B);
