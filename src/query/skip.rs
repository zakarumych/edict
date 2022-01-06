use crate::{archetype::Archetype, proof::Skip};

use super::{Fetch, ImmutableQuery, NonTrackingQuery, Query};

impl Fetch<'_> for Skip {
    type Item = Skip;
    type Chunk = Skip;

    #[inline]
    unsafe fn get_chunk(&mut self, _: usize) -> Skip {
        Skip
    }

    #[inline]
    unsafe fn get_item(_: &Skip, _: usize) -> Skip {
        Skip
    }

    unsafe fn get_one_item(&mut self, _: u32) -> Skip {
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
    unsafe fn fetch(_: &Archetype, _: u64, _epoch: u64) -> Option<Skip> {
        Some(Skip)
    }
}

unsafe impl ImmutableQuery for Skip {}
unsafe impl NonTrackingQuery for Skip {}
