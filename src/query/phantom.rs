use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::{fetch::Fetch, Access, ImmutableQuery, IntoQuery, Query};

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

    /// Returns what kind of access the query performs on the component type.
    fn access(ty: TypeId) -> Option<Access>;

    /// Checks if archetype must be skipped.
    fn skip_archetype(archetype: &Archetype) -> bool;

    /// Asks query to provide types and access for the specific archetype.
    /// Must call provided closure with type id and access pairs.
    /// For each `(id, access)` pair access must match one returned from `access` method for the same id.
    /// Only types from archetype must be used to call closure.
    unsafe fn access_archetype(_archetype: &Archetype, f: &dyn Fn(TypeId, Access));

    /// Fetches data from one archetype.
    ///
    /// # Safety
    ///
    /// Must not be called if `skip_archetype` returned `true`.
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: EpochId) -> Self::Fetch<'a>;
}

impl<Q> IntoQuery for PhantomData<fn() -> Q>
where
    Q: PhantomQuery,
{
    type Query = PhantomData<fn() -> Q>;
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
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        <Q as PhantomQuery>::skip_archetype(archetype)
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        <Q as PhantomQuery>::access_archetype(archetype, f)
    }

    #[inline]
    unsafe fn fetch<'a>(&mut self, archetype: &'a Archetype, epoch: EpochId) -> Self::Fetch<'a> {
        <Q as PhantomQuery>::fetch(archetype, epoch)
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
    #[inline]
    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        <T as PhantomQuery>::skip_archetype(archetype)
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
