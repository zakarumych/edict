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
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) -> bool {
        self.filter.visit_chunk(chunk_idx) && self.query.visit_chunk(chunk_idx)
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: usize) {
        self.filter.touch_chunk(chunk_idx);
        self.query.touch_chunk(chunk_idx);
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: usize) -> bool {
        self.filter.visit_item(idx) && self.query.visit_item(idx)
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
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.filter.visit_archetype(archetype) && self.query.visit_archetype(archetype)
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

/// Inverse of a filter.
/// Entities that match the filter are skipped.
///
/// The `Not` filter will NOT cause side effects of the inner filter.
pub struct Not<T>(pub T);

pub struct NotFetch<T>(T, bool);

unsafe impl<'a, T> Fetch<'a> for NotFetch<T>
where
    T: Fetch<'a>,
{
    type Item = ();

    fn dangling() -> Self {
        NotFetch(T::dangling(), false)
    }

    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) -> bool {
        self.1 = self.0.visit_chunk(chunk_idx);
        true
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, _chunk_idx: usize) {}

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: usize) -> bool {
        if self.1 {
            !self.0.visit_item(idx)
        } else {
            true
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, _idx: usize) {}
}

impl<T> IntoQuery for Not<T>
where
    T: IntoQuery,
{
    type Query = Not<T::Query>;
}

unsafe impl<T> Query for Not<T>
where
    T: Query,
{
    type Item<'a> = ();
    type Fetch<'a> = NotFetch<T::Fetch<'a>>;

    #[inline]
    fn access(&self, _: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        !self.0.visit_archetype(archetype)
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> NotFetch<T::Fetch<'a>> {
        NotFetch(self.0.fetch(archetype, epoch), false)
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
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(_: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutablePhantomQuery for With<T> where T: 'static {}

/// [`Filter`] that allows only archetypes without specified component.
/// Inverse of [`With`].
pub type Without<T> = Not<With<T>>;

/// Binary operator for [`BooleanFilter`].
pub trait BinOp: 'static {
    /// The boolean binary operator.
    fn op(a: bool, b: bool) -> bool;
}

pub enum AndOp {}

impl BinOp for AndOp {
    fn op(a: bool, b: bool) -> bool {
        a && b
    }
}

pub enum OrOp {}

impl BinOp for OrOp {
    fn op(a: bool, b: bool) -> bool {
        a || b
    }
}

pub enum XorOp {}

impl BinOp for XorOp {
    fn op(a: bool, b: bool) -> bool {
        a ^ b
    }
}

pub enum NandOp {}

impl BinOp for NandOp {
    fn op(a: bool, b: bool) -> bool {
        !(a && b)
    }
}

/// Combines two filters and yields only entities that pass both.
pub struct BooleanFilter<A, B, Op>(pub A, pub B, PhantomData<Op>);

impl<A, B, Op> BooleanFilter<A, B, Op> {
    /// Construct a new [`BooleanFilter`].
    pub fn new(a: A, b: B) -> Self {
        BooleanFilter(a, b, PhantomData)
    }
}

/// Boolean filter combines two filters and boolean operation.
pub struct BooleanFetch<A, B, Op>(A, B, PhantomData<Op>);

unsafe impl<'a, A, B, Op> Fetch<'a> for BooleanFetch<A, B, Op>
where
    A: Fetch<'a>,
    B: Fetch<'a>,
    Op: BinOp,
{
    type Item = ();

    #[inline(always)]
    fn dangling() -> Self {
        BooleanFetch(A::dangling(), B::dangling(), PhantomData)
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, _idx: usize) {}

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: usize) -> bool {
        Op::op(self.0.visit_item(idx), self.1.visit_item(idx))
    }

    #[inline(always)]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) -> bool {
        Op::op(self.0.visit_chunk(chunk_idx), self.1.visit_chunk(chunk_idx))
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: usize) {
        self.0.touch_chunk(chunk_idx);
        self.1.touch_chunk(chunk_idx);
    }
}

impl<A, B, Op> IntoQuery for BooleanFilter<A, B, Op>
where
    A: IntoQuery,
    B: IntoQuery,
    Op: BinOp,
{
    type Query = BooleanFilter<A::Query, B::Query, Op>;
}

unsafe impl<A, B, Op> Query for BooleanFilter<A, B, Op>
where
    A: Query,
    B: Query,
    Op: BinOp,
{
    type Item<'a> = ();
    type Fetch<'a> = BooleanFetch<A::Fetch<'a>, B::Fetch<'a>, Op>;

    fn access(&self, ty: TypeId) -> Option<Access> {
        merge_access(self.0.access(ty), self.1.access(ty))
    }

    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        self.0.access_archetype(archetype, f);
        self.1.access_archetype(archetype, f);
    }

    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.0.visit_archetype(archetype) && self.1.visit_archetype(archetype)
    }

    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> BooleanFetch<A::Fetch<'a>, B::Fetch<'a>, Op> {
        BooleanFetch(
            self.0.fetch(archetype, epoch),
            self.1.fetch(archetype, epoch),
            PhantomData,
        )
    }
}

/// Combines two filters and yields only entities that pass both.
pub type And<A, B> = BooleanFilter<A, B, AndOp>;

/// Combines two filters and yields only entities that pass either.
pub type Or<A, B> = BooleanFilter<A, B, OrOp>;

/// Combines two filters and yields only entities that pass either, but not both.
pub type Xor<A, B> = BooleanFilter<A, B, XorOp>;
