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
pub trait BooleanMonoid: 'static {
    /// The boolean monoid identity.
    fn identity() -> bool;

    /// The boolean binary operator.
    fn op(a: bool, b: bool) -> Result<bool, bool>;
}

pub enum AndOp {}

impl BooleanMonoid for AndOp {
    fn identity() -> bool {
        true
    }

    fn op(a: bool, b: bool) -> Result<bool, bool> {
        if a && b {
            Ok(true)
        } else {
            Err(false)
        }
    }
}

pub enum OrOp {}

impl BooleanMonoid for OrOp {
    fn identity() -> bool {
        false
    }

    fn op(a: bool, b: bool) -> Result<bool, bool> {
        if a || b {
            Err(true)
        } else {
            Ok(false)
        }
    }
}

pub enum XorOp {}

impl BooleanMonoid for XorOp {
    fn identity() -> bool {
        false
    }

    fn op(a: bool, b: bool) -> Result<bool, bool> {
        match (a, b) {
            (false, false) => Ok(false),
            (true, true) => Err(false),
            _ => Ok(true),
        }
    }
}

/// Combines two filters and yields only entities that pass both.
pub struct BooleanFilter<T, Op>(T, PhantomData<Op>);

impl<T, Op> BooleanFilter<T, Op> {
    /// Creates a new [`BooleanFilter`].
    pub fn from_tuple(tuple: T) -> Self {
        Self(tuple, PhantomData)
    }
}

/// Boolean filter combines two filters and boolean operation.
pub struct BooleanFetch<T, Op>(T, PhantomData<Op>);

pub struct BooleanFetchElem<T>(T, bool, bool);

macro_rules! impl_boolean {
    ($($a:ident)*) => {
        #[allow(non_snake_case)]
        #[allow(unused_variables, unused_mut)]
        unsafe impl<'a, Op $(, $a)*> Fetch<'a> for BooleanFetch<($(BooleanFetchElem<$a>,)*), Op>
        where
            $($a: Fetch<'a>,)*
            Op: BooleanMonoid,
        {
            type Item = ();

            #[inline(always)]
            fn dangling() -> Self {
                BooleanFetch(($(BooleanFetchElem($a::dangling(), false, false),)*), PhantomData)
            }

            #[inline(always)]
            unsafe fn get_item(&mut self, _idx: usize) {}

            #[inline(always)]
            unsafe fn visit_item(&mut self, idx: usize) -> bool {
                let ($($a,)*) = &mut self.0;
                let mut result = Op::identity();
                $(
                    if $a.2 {
                        match Op::op(result, $a.0.visit_item(idx)) {
                            Ok(ok) => result = ok,
                            Err(err) => return err,
                        }
                    }
                )*
                result
            }

            #[inline(always)]
            unsafe fn visit_chunk(&mut self, chunk_idx: usize) -> bool {
                let ($($a,)*) = &mut self.0;
                let mut result = Op::identity();
                $(
                    if $a.1 {
                        $a.2 = $a.0.visit_chunk(chunk_idx);
                        match Op::op(result, $a.2) {
                            Ok(ok) => result = ok,
                            Err(err) => return err,
                        }
                    }
                )*
                result
            }

            #[inline(always)]
            unsafe fn touch_chunk(&mut self, _chunk_idx: usize) {}
        }

        #[allow(non_snake_case)]
        impl<'a, Op $(, $a)*> BooleanFilter<($($a,)*), Op>
        where
            Op: BooleanMonoid,
        {
            /// Creates a new [`BooleanFilter`].
            #[inline(always)]
            pub fn new($($a: $a),*) -> Self {
                BooleanFilter(($($a,)*), PhantomData)
            }
        }

        #[allow(non_snake_case)]
        impl<Op $(, $a)*> IntoQuery for BooleanFilter<($($a,)*), Op>
        where
            $($a: IntoQuery,)*
            Op: BooleanMonoid,
        {
            type Query = BooleanFilter<($($a::Query,)*), Op>;
        }

        #[allow(non_snake_case)]
        #[allow(unused_variables, unused_mut)]
        unsafe impl<Op $(, $a)*> Query for BooleanFilter<($($a,)*), Op>
        where
            $($a: Query,)*
            Op: BooleanMonoid,
        {
            type Item<'a> = ();
            type Fetch<'a> = BooleanFetch<($(BooleanFetchElem<$a::Fetch<'a>>,)*), Op>;

            #[inline(always)]
            fn access(&self, ty: TypeId) -> Option<Access> {
                let ($($a,)*) = &self.0;
                let mut result = None;
                $(result = merge_access(result, $a.access(ty));)*
                result
            }

            #[inline(always)]
            unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
                let ($($a,)*) = &self.0;
                $($a.access_archetype(archetype, f);)*
            }

            #[inline(always)]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)*) = &self.0;
                let mut result = Op::identity();
                $({
                    let visit = $a.visit_archetype(archetype);
                    match Op::op(result, visit) {
                        Ok(ok) => result = ok,
                        Err(err) => return err,
                    }
                })*
                result
            }

            #[inline(always)]
            unsafe fn fetch<'a>(
                &mut self,
                archetype: &'a Archetype,
                epoch: EpochId,
            ) -> BooleanFetch<($(BooleanFetchElem<$a::Fetch<'a>>,)*), Op> {
                let ($($a,)*) = &mut self.0;
                BooleanFetch(
                    ($({
                        let visit = $a.visit_archetype(archetype);
                        let fetch = if visit {
                            $a.fetch(archetype, epoch)
                        } else {
                            Fetch::dangling()
                        };
                        BooleanFetchElem(fetch, visit, false)
                    },)*),
                    PhantomData,
                )
            }
        }

        unsafe impl<Op $(, $a)*> ImmutableQuery for BooleanFilter<($($a,)*), Op>
        where
            $($a: ImmutableQuery,)*
            Op: BooleanMonoid,
        {
        }
    };
}

for_tuple!(impl_boolean);

/// Combines tuple of filters and yields only entities that pass all of them.
pub type And<T> = BooleanFilter<T, AndOp>;

/// Combines tuple of filters and yields only entities that pass any of them.
pub type Or<T> = BooleanFilter<T, OrOp>;

/// Combines tuple of filters and yields only entities that pass exactly one.
pub type Xor<T> = BooleanFilter<T, XorOp>;

/// Combines two filters and yields only entities that pass all of them.
pub type And2<A, B> = And<(A, B)>;

/// Combines three filters and yields only entities that pass all of them.
pub type And3<A, B, C> = And<(A, B, C)>;

/// Combines four filters and yields only entities that pass all of them.
pub type And4<A, B, C, D> = And<(A, B, C, D)>;

/// Combines five filters and yields only entities that pass all of them.
pub type And5<A, B, C, D, E> = And<(A, B, C, D, E)>;

/// Combines six filters and yields only entities that pass all of them.
pub type And6<A, B, C, D, E, F> = And<(A, B, C, D, E, F)>;

/// Combines seven filters and yields only entities that pass all of them.
pub type And7<A, B, C, D, E, F, G> = And<(A, B, C, D, E, F, G)>;

/// Combines eight filters and yields only entities that pass all of them.
pub type And8<A, B, C, D, E, F, G, H> = And<(A, B, C, D, E, F, G, H)>;

/// Combines two filters and yields only entities that pass any of them.
pub type Or2<A, B> = Or<(A, B)>;

/// Combines three filters and yields only entities that pass any of them.
pub type Or3<A, B, C> = Or<(A, B, C)>;

/// Combines four filters and yields only entities that pass any of them.
pub type Or4<A, B, C, D> = Or<(A, B, C, D)>;

/// Combines five filters and yields only entities that pass any of them.
pub type Or5<A, B, C, D, E> = Or<(A, B, C, D, E)>;

/// Combines six filters and yields only entities that pass any of them.
pub type Or6<A, B, C, D, E, F> = Or<(A, B, C, D, E, F)>;

/// Combines seven filters and yields only entities that pass any of them.
pub type Or7<A, B, C, D, E, F, G> = Or<(A, B, C, D, E, F, G)>;

/// Combines eight filters and yields only entities that pass any of them.
pub type Or8<A, B, C, D, E, F, G, H> = Or<(A, B, C, D, E, F, G, H)>;

/// Combines two filters and yields only entities that pass exactly one.
pub type Xor2<A, B> = Xor<(A, B)>;

/// Combines three filters and yields only entities that pass exactly one.
pub type Xor3<A, B, C> = Xor<(A, B, C)>;

/// Combines four filters and yields only entities that pass exactly one.
pub type Xor4<A, B, C, D> = Xor<(A, B, C, D)>;

/// Combines five filters and yields only entities that pass exactly one.
pub type Xor5<A, B, C, D, E> = Xor<(A, B, C, D, E)>;

/// Combines six filters and yields only entities that pass exactly one.
pub type Xor6<A, B, C, D, E, F> = Xor<(A, B, C, D, E, F)>;

/// Combines seven filters and yields only entities that pass exactly one.
pub type Xor7<A, B, C, D, E, F, G> = Xor<(A, B, C, D, E, F, G)>;

/// Combines eight filters and yields only entities that pass exactly one.
pub type Xor8<A, B, C, D, E, F, G, H> = Xor<(A, B, C, D, E, F, G, H)>;
