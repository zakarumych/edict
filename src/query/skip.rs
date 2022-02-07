use crate::{archetype::Archetype, proof::Skip};

use super::{Fetch, ImmutableQuery, NonTrackingQuery, Query};

impl Fetch<'_> for Skip {
    type Item = Skip;

    #[inline]
    fn dangling() -> Self {
        Skip
    }

    #[inline]
    unsafe fn get_item(&mut self, _: usize) -> Skip {
        Skip
    }
}

impl Query for Skip {
    type Fetch = Skip;

    #[inline]
    fn mutates() -> bool {
        false
    }

    #[inline]
    fn skip_archetype(_: &Archetype, _: u64) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(_: &Archetype, _: u64, _epoch: u64) -> Option<Skip> {
        Some(Skip)
    }
}

unsafe impl ImmutableQuery for Skip {}
unsafe impl NonTrackingQuery for Skip {}
