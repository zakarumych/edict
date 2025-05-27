use core::any::TypeId;

use crate::{
    archetype::Archetype, component::ComponentInfo, epoch::EpochId, system::QueryArg, type_id,
};

use super::{
    fetch::UnitFetch, Access, AsQuery, BatchFetch, DefaultQuery, Fetch, ImmutableQuery, IntoQuery,
    Query, SendQuery, WriteAlias,
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

    #[inline]
    fn dangling() -> Self {
        FilteredFetch {
            filter: F::dangling(),
            query: Q::dangling(),
        }
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        unsafe { self.filter.visit_chunk(chunk_idx) && self.query.visit_chunk(chunk_idx) }
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        unsafe {
            self.filter.touch_chunk(chunk_idx);
        }
        unsafe {
            self.query.touch_chunk(chunk_idx);
        }
    }

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        unsafe { self.filter.visit_item(idx) && self.query.visit_item(idx) }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> Self::Item {
        unsafe { self.query.get_item(idx) }
    }
}

// /// Combines query and filter.
// /// Skips using both and yields using query.
// #[derive(Clone, Copy, Debug)]
// pub struct FilteredQuery<F, Q> {
//     pub(crate) filter: F,
//     pub(crate) query: Q,
// }

// impl<F, Q> IntoQuery for FilteredQuery<F, Q>
// where
//     F: IntoQuery,
//     Q: IntoQuery,
// {
//     type Query = FilteredQuery<F::Query, Q::Query>;

//     #[inline]
//     fn into_query(self) -> Self::Query {
//         FilteredQuery {
//             filter: self.filter.into_query(),
//             query: self.query.into_query(),
//         }
//     }
// }

// unsafe impl<F, Q> Query for FilteredQuery<F, Q>
// where
//     F: Query,
//     Q: Query,
// {
//     type Item<'a> = Q::Item<'a> where Q: 'a;
//     type Fetch<'a> = FilteredFetch<F::Fetch<'a>, Q::Fetch<'a>> where F: 'a, Q: 'a;

//     const MUTABLE: bool = F::MUTABLE || Q::MUTABLE;

//     #[inline]
//     fn component_access(&self, comp: &ComponentInfo) -> Option<Access> {
//         match (
//             self.filter.component_access(ty),
//             self.query.component_access(ty),
//         ) {
//             (None, one) | (one, None) => one,
//             (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
//             (Some(Access::Write), _) | (_, Some(Access::Write)) => {
//                 panic!(
//                     "Conflicting query and filter in `{}`.
//                         A component is aliased mutably.",
//                     core::any::type_name::<Self>()
//                 );
//             }
//         }
//     }

//     #[inline]
//     fn visit_archetype(&self, archetype: &Archetype) -> bool {
//         self.filter.visit_archetype(archetype) && self.query.visit_archetype(archetype)
//     }

//     #[inline]
//     unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

//     #[inline]
//     unsafe fn fetch<'a>(
//         &self,
//         arch_idx: u32,
//         archetype: &'a Archetype,
//         index: EpochId,
//     ) -> FilteredFetch<F::Fetch<'a>, Q::Fetch<'a>> {
//         FilteredFetch {
//             filter: self.filter.fetch(arch_idx, archetype, index),
//             query: self.query.fetch(arch_idx, archetype, index),
//         }
//     }
// }

// unsafe impl<F, Q> ImmutableQuery for FilteredQuery<F, Q>
// where
//     Q: ImmutableQuery,
//     F: ImmutableQuery,
// {
// }

/// Inverse of a filter.
/// Entities that match the filter are skipped.
///
/// The `Not` filter will NOT cause side effects of the inner filter.
#[derive(Clone, Copy)]
pub struct Not<T>(pub T);

pub enum NotFetch<T> {
    Fetch { fetch: T, visit_chunk: bool },
    None,
}

unsafe impl<'a, T> Fetch<'a> for NotFetch<T>
where
    T: Fetch<'a>,
{
    type Item = ();

    #[inline]
    fn dangling() -> Self {
        NotFetch::None
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
        match self {
            NotFetch::Fetch { fetch, visit_chunk } => {
                *visit_chunk = unsafe { fetch.visit_chunk(chunk_idx) }
            }
            NotFetch::None => {}
        }
        true
    }

    #[inline]
    unsafe fn touch_chunk(&mut self, _chunk_idx: u32) {}

    #[inline]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        match self {
            NotFetch::Fetch { fetch, visit_chunk } if *visit_chunk => unsafe {
                fetch.visit_item(idx)
            },
            _ => true,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, _idx: u32) {}
}

unsafe impl<'a, T> BatchFetch<'a> for NotFetch<T>
where
    T: BatchFetch<'a>,
{
    type Batch = ();

    #[inline]
    unsafe fn get_batch(&mut self, _start: u32, _end: u32) {}
}

impl<T> AsQuery for Not<T>
where
    T: AsQuery,
{
    type Query = Not<T::Query>;
}

impl<T> IntoQuery for Not<T>
where
    T: IntoQuery,
{
    #[inline]
    fn into_query(self) -> Not<T::Query> {
        Not(self.0.into_query())
    }
}

impl<T> DefaultQuery for Not<T>
where
    T: DefaultQuery,
{
    #[inline]
    fn default_query() -> Not<T::Query> {
        Not(T::default_query())
    }
}

impl<T> QueryArg for Not<T>
where
    T: QueryArg,
{
    #[inline]
    fn new() -> Not<T::Query> {
        Not(T::new())
    }
}

unsafe impl<T> Query for Not<T>
where
    T: Query,
{
    type Item<'a> = ();
    type Fetch<'a>
        = NotFetch<T::Fetch<'a>>
    where
        T: 'a;

    const MUTABLE: bool = T::MUTABLE;

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        if T::FILTERS_ENTITIES {
            true
        } else {
            !self.0.visit_archetype(archetype)
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> NotFetch<T::Fetch<'a>> {
        if self.0.visit_archetype(archetype) {
            NotFetch::Fetch {
                fetch: unsafe { self.0.fetch(arch_idx, archetype, epoch) },
                visit_chunk: false,
            }
        } else {
            NotFetch::None
        }
    }
}

unsafe impl<T> ImmutableQuery for Not<T> where T: ImmutableQuery {}
unsafe impl<T> SendQuery for Not<T> where T: SendQuery {}

marker_type! {
    /// [`Query`] that allows only archetypes with specified component.
    pub struct With<T>;
}

impl<T> AsQuery for With<T>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for With<T>
where
    T: 'static,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for With<T>
where
    T: 'static,
{
    #[inline]
    fn default_query() -> Self {
        With
    }
}

impl<T> QueryArg for With<T>
where
    T: 'static,
{
    #[inline]
    fn new() -> With<T> {
        With
    }
}

unsafe impl<T> Query for With<T>
where
    T: 'static,
{
    type Item<'a> = ();
    type Fetch<'a> = UnitFetch;

    const MUTABLE: bool = false;

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        Ok(None)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(type_id::<T>())
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, _f: impl FnMut(TypeId, Access)) {}

    #[inline]
    unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> UnitFetch {
        UnitFetch::new()
    }
}

unsafe impl<T> ImmutableQuery for With<T> where T: 'static {}
unsafe impl<T> SendQuery for With<T> where T: 'static {}

/// [`Query`] that allows only archetypes without specified component.
/// Inverse of [`With`].
pub type Without<T> = Not<With<T>>;
