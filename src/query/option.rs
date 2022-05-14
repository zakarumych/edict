use core::any::TypeId;

use crate::archetype::Archetype;

use super::{Access, Fetch, ImmutableQuery, NonTrackingQuery, Query};

impl<'a, T> Fetch<'a> for Option<T>
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
    unsafe fn skip_chunk(&self, chunk_idx: usize) -> bool {
        if let Some(fetch) = self {
            fetch.skip_chunk(chunk_idx)
        } else {
            false
        }
    }

    /// Checks if item with specified index must be skipped.
    #[inline]
    unsafe fn skip_item(&self, idx: usize) -> bool {
        if let Some(fetch) = self {
            fetch.skip_item(idx)
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

    /// Returns fetched item at specifeid index.
    unsafe fn get_item(&mut self, idx: usize) -> Option<T::Item> {
        match self {
            None => None,
            Some(fetch) => Some(fetch.get_item(idx)),
        }
    }
}

unsafe impl<T> Query for Option<T>
where
    T: Query,
{
    type Fetch = Option<T::Fetch>;

    #[inline]
    fn mutates() -> bool {
        T::mutates()
    }

    #[inline]
    fn access(ty: TypeId) -> Access {
        <T as Query>::access(ty)
    }

    #[inline]
    fn allowed_with<Q: Query>() -> bool {
        <T as Query>::allowed_with::<Q>()
    }

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn skip_archetype(_: &Archetype, _: u64) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, tracks: u64, epoch: u64) -> Option<Option<T::Fetch>> {
        Some(<T as Query>::fetch(archetype, tracks, epoch))
    }
}

unsafe impl<T> ImmutableQuery for Option<T> where T: ImmutableQuery {}
unsafe impl<T> NonTrackingQuery for Option<T> where T: NonTrackingQuery {}

// impl<'a, T> Fetch<'a> for Option<FetchRead<T>>
// where
//     T: Component,
// {
//     type Item = Option<&'a T>;

//     #[inline]
//     fn dangling() -> Self {
//         None
//     }

//     #[inline]
//     unsafe fn get_item(&mut self, idx: usize) -> Option<&'a T> {
//         Some(self.as_mut()?.get_item(idx))
//     }
// }

// impl<T> Query for Option<&T>
// where
//     T: Component,
// {
//     type Fetch = Option<FetchRead<T>>;

//     #[inline]
//     fn mutates() -> bool {
//         false
//     }

//     #[inline]
//     unsafe fn fetch(
//         archetype: &Archetype,
//         tracks: u64,
//         epoch: u64,
//     ) -> Option<Option<FetchRead<T>>> {
//         Some(<&T as Query>::fetch(archetype, tracks, epoch))
//     }
// }

// unsafe impl<T> ImmutableQuery for Option<&T> where T: Component {}
// unsafe impl<T> NonTrackingQuery for Option<&T> where T: Component {}

// impl<'a, T> Fetch<'a> for Option<FetchWrite<T>>
// where
//     T: Component,
// {
//     type Item = Option<&'a mut T>;

//     #[inline]
//     fn dangling() -> Self {
//         None
//     }

//     #[inline]
//     unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
//         if let Some(fetch) = self.as_mut() {
//             fetch.visit_chunk(chunk_idx)
//         }
//     }

//     #[inline]
//     unsafe fn get_item(&mut self, idx: usize) -> Option<&'a mut T> {
//         Some(self.as_mut()?.get_item(idx))
//     }
// }

// impl<T> Query for Option<&mut T>
// where
//     T: Component,
// {
//     type Fetch = Option<FetchWrite<T>>;

//     #[inline]
//     fn mutates() -> bool {
//         true
//     }

//     #[inline]
//     unsafe fn fetch(
//         archetype: &Archetype,
//         track: u64,
//         epoch: u64,
//     ) -> Option<Option<FetchWrite<T>>> {
//         Some(<&mut T as Query>::fetch(archetype, track, epoch))
//     }
// }

// unsafe impl<T> NonTrackingQuery for Option<&mut T> where T: Component {}

// impl<'a, T> Fetch<'a> for Option<FetchAlt<T>>
// where
//     T: Component,
// {
//     type Item = Option<RefMut<'a, T>>;

//     #[inline]
//     fn dangling() -> Self {
//         None
//     }

//     #[inline]
//     unsafe fn get_item(&mut self, idx: usize) -> Option<RefMut<'a, T>> {
//         Some(self.as_mut()?.get_item(idx))
//     }
// }

// impl<T> Query for Option<Alt<T>>
// where
//     T: Component,
// {
//     type Fetch = Option<FetchAlt<T>>;

//     #[inline]
//     fn mutates() -> bool {
//         true
//     }

//     #[inline]
//     unsafe fn fetch(archetype: &Archetype, track: u64, epoch: u64) -> Option<Option<FetchAlt<T>>> {
//         Some(<Alt<T> as Query>::fetch(archetype, track, epoch))
//     }
// }

// unsafe impl<T> NonTrackingQuery for Option<Alt<T>> where T: Component {}
