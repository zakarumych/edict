use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::{fetch::Fetch, Access, DefaultQuery, ImmutableQuery, IntoQuery, Query};

/// Phantom counterpart of [`Query`] trait.
/// This trait has all the same methods without `self` argument.
///
/// [`PhantomData<fn() -> Q>`] implements [`Query`] trait if `Q` implements [`Query`] trait.
pub unsafe trait PhantomQuery: IntoQuery<Query = PhantomData<fn() -> Self>> {
    /// Item type this query type yields.
    type Item<'a>: 'a;

    /// Fetch value type for this query type.
    /// Contains data from one archetype.
    type Fetch<'a>: Fetch<'a, Item = Self::Item<'a>> + 'a;

    /// Constructs the query instance.
    #[must_use]
    #[inline(always)]
    fn query() -> PhantomData<fn() -> Self> {
        PhantomData
    }

    /// Returns what kind of access the query performs on the component type.
    #[must_use]
    fn access(ty: TypeId) -> Option<Access>;

    /// Checks if archetype must be visited or skipped.
    #[must_use]
    fn visit_archetype(archetype: &Archetype) -> bool;

    /// Asks query to provide types and access for the specific archetype.
    /// Must call provided closure with type id and access pairs.
    /// For each `(id, access)` pair access must match one returned from `access` method for the same id.
    /// Only types from archetype must be used to call closure.
    unsafe fn access_archetype(archetype: &Archetype, f: &dyn Fn(TypeId, Access));

    /// Fetches data from one archetype.
    ///
    /// # Safety
    ///
    /// Must not be called if `skip_archetype` returned `true`.
    #[must_use]
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: EpochId) -> Self::Fetch<'a>;

    /// Returns item for reserved entity if reserved entity satisfies the query.
    /// Otherwise returns `None`.
    #[must_use]
    #[inline]
    fn reserved_entity_item<'a>(id: EntityId, idx: u32) -> Option<Self::Item<'a>> {
        drop(id);
        None
    }
}

impl<Q> IntoQuery for PhantomData<fn() -> Q>
where
    Q: PhantomQuery,
{
    type Query = Self;

    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<Q> IntoQuery for Q
where
    Q: PhantomQuery,
{
    type Query = PhantomData<fn() -> Q>;

    #[inline]
    fn into_query(self) -> Self::Query {
        PhantomData
    }
}

impl<Q> DefaultQuery for Q
where
    Q: PhantomQuery,
{
    #[inline]
    fn default_query() -> Self::Query {
        PhantomData
    }
}

unsafe impl<Q> Query for PhantomData<fn() -> Q>
where
    Q: PhantomQuery,
{
    type Item<'a> = Q::Item<'a>;
    type Fetch<'a> = Q::Fetch<'a>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Q as PhantomQuery>::access(ty)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        <Q as PhantomQuery>::visit_archetype(archetype)
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        <Q as PhantomQuery>::access_archetype(archetype, f)
    }

    #[inline]
    unsafe fn fetch<'a>(&mut self, archetype: &'a Archetype, epoch: EpochId) -> Self::Fetch<'a> {
        <Q as PhantomQuery>::fetch(archetype, epoch)
    }

    #[inline]
    fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<Self::Item<'a>> {
        <Q as PhantomQuery>::reserved_entity_item(id, idx)
    }
}

/// Phantom counterpart of [`ImmutableQuery`] type alias.
pub unsafe trait ImmutablePhantomQuery: PhantomQuery {}

unsafe impl<Q> ImmutableQuery for PhantomData<fn() -> Q> where Q: ImmutablePhantomQuery {}

impl<'a, T> QueryArgGet<'a> for PhantomData<fn() -> T>
where
    T: PhantomQuery + 'static,
{
    type Arg = T;
    type Query = PhantomData<fn() -> T>;

    #[inline]
    fn get(&mut self, _world: &World) -> Self::Query {
        PhantomData
    }
}

impl<T> QueryArgCache for PhantomData<fn() -> T>
where
    T: PhantomQuery + 'static,
{
    fn new() -> Self {
        PhantomData
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        <T as PhantomQuery>::visit_archetype(archetype)
    }

    #[inline]
    fn access_component(&self, id: TypeId) -> Option<Access> {
        <T as PhantomQuery>::access(id)
    }
}

impl<T> QueryArg for T
where
    T: PhantomQuery + 'static,
{
    type Cache = PhantomData<fn() -> T>;
}
