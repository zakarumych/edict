use crate::{archetype::Archetype, component::Component};

use super::{
    alt::{Alt, FetchAlt, RefMut},
    read::FetchRead,
    write::FetchWrite,
    Fetch, ImmutableQuery, NonTrackingQuery, Query,
};

impl<'a, T> Fetch<'a> for Option<FetchRead<T>>
where
    T: Component,
{
    type Item = Option<&'a T>;

    #[inline]
    fn dangling() -> Self {
        None
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> Option<&'a T> {
        Some(self.as_mut()?.get_item(idx))
    }
}

impl<T> Query for Option<&T>
where
    T: Component,
{
    type Fetch = Option<FetchRead<T>>;

    #[inline]
    fn mutates() -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(
        archetype: &Archetype,
        tracks: u64,
        epoch: u64,
    ) -> Option<Option<FetchRead<T>>> {
        Some(<&T as Query>::fetch(archetype, tracks, epoch))
    }
}

unsafe impl<T> ImmutableQuery for Option<&T> where T: Component {}
unsafe impl<T> NonTrackingQuery for Option<&T> where T: Component {}

impl<'a, T> Fetch<'a> for Option<FetchWrite<T>>
where
    T: Component,
{
    type Item = Option<&'a mut T>;

    #[inline]
    fn dangling() -> Self {
        None
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        if let Some(fetch) = self.as_mut() {
            fetch.visit_chunk(chunk_idx)
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> Option<&'a mut T> {
        Some(self.as_mut()?.get_item(idx))
    }
}

impl<T> Query for Option<&mut T>
where
    T: Component,
{
    type Fetch = Option<FetchWrite<T>>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(
        archetype: &Archetype,
        track: u64,
        epoch: u64,
    ) -> Option<Option<FetchWrite<T>>> {
        Some(<&mut T as Query>::fetch(archetype, track, epoch))
    }
}

unsafe impl<T> NonTrackingQuery for Option<&mut T> where T: Component {}

impl<'a, T> Fetch<'a> for Option<FetchAlt<T>>
where
    T: Component,
{
    type Item = Option<RefMut<'a, T>>;

    #[inline]
    fn dangling() -> Self {
        None
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> Option<RefMut<'a, T>> {
        Some(self.as_mut()?.get_item(idx))
    }
}

impl<T> Query for Option<Alt<T>>
where
    T: Component,
{
    type Fetch = Option<FetchAlt<T>>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, track: u64, epoch: u64) -> Option<Option<FetchAlt<T>>> {
        Some(<Alt<T> as Query>::fetch(archetype, track, epoch))
    }
}

unsafe impl<T> NonTrackingQuery for Option<Alt<T>> where T: Component {}
