use core::{any::TypeId, marker::PhantomData};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{Access, Fetch, ImmutablePhantomQuery, IntoQuery, PhantomQuery, PhantomQueryFetch};

unsafe impl<'a, T> Fetch<'a> for Option<T>
where
    T: Fetch<'a>,
{
    type Item = Option<T::Item>;

    /// Returns `Fetch` value that must not be used.
    fn dangling() -> Self {
        None
    }

    /// Checks if chunk with specified index must be skipped.
    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        if let Some(fetch) = self {
            fetch.skip_chunk(chunk_idx)
        } else {
            false
        }
    }

    /// Notifies this fetch that it visits a chunk.
    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        if let Some(fetch) = self {
            fetch.visit_chunk(chunk_idx);
        }
    }

    /// Checks if item with specified index must be skipped.
    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        if let Some(fetch) = self {
            fetch.skip_item(idx)
        } else {
            false
        }
    }

    /// Returns fetched item at specified index.
    unsafe fn get_item(&mut self, idx: usize) -> Option<T::Item> {
        match self {
            None => None,
            Some(fetch) => Some(fetch.get_item(idx)),
        }
    }
}

impl<T> IntoQuery for Option<T>
where
    T: PhantomQuery,
{
    type Query = PhantomData<Option<T>>;
}

impl<'a, T> PhantomQueryFetch<'a> for Option<T>
where
    T: PhantomQuery,
{
    type Item = Option<<T as PhantomQueryFetch<'a>>::Item>;
    type Fetch = Option<<T as PhantomQueryFetch<'a>>::Fetch>;
}

impl<T> PhantomQuery for Option<T>
where
    T: PhantomQuery,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        T::access(ty)
    }

    #[inline]
    fn skip_archetype(_: &Archetype) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch<'a>(
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Option<<T as PhantomQueryFetch<'a>>::Fetch> {
        if T::skip_archetype(archetype) {
            None
        } else {
            Some(T::fetch(archetype, epoch))
        }
    }
}

unsafe impl<T> ImmutablePhantomQuery for Option<T> where T: ImmutablePhantomQuery {}

// impl<T> QueryArg for Option<T>
// where
//     T: PhantomQuery + QueryArg,
//     for<'a> <T::Cache as QueryArgGet<'a>>::Arg: PhantomQuery,
// {
//     type Cache = PhantomData<Option<T::Cache>>;
// }

// impl<'a, T> QueryArgGet<'a> for PhantomData<Option<T>>
// where
//     T: QueryArgCache,
//     for<'b> <T as QueryArgGet<'b>>::Arg: PhantomQuery,
// {
//     type Arg = Option<<T as QueryArgGet<'a>>::Arg>;

//     /// Constructed query type.
//     type Query = PhantomData<Option<<T as QueryArgGet<'a>>::Arg>>;

//     /// Returns query for an argument.
//     fn get(&mut self, _world: &World) -> PhantomData<Option<<T as QueryArgGet<'a>>::Arg>> {
//         PhantomData
//     }
// }

// impl<T> QueryArgCache for PhantomData<Option<T>>
// where
//     T: QueryArgCache,
//     for<'a> <T as QueryArgGet<'a>>::Arg: PhantomQuery,
// {
//     fn skips_archetype(&self, _archetype: &Archetype) -> bool {
//         false
//     }

//     fn access_component(&self, id: TypeId) -> Option<Access> {
//         <<T as QueryArgGet<'static>>::Arg as PhantomQuery>::access(id)
//     }
// }
