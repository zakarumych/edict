use core::any::TypeId;

use crate::{archetype::Archetype, proof::Skip};

use super::{Access, Fetch, ImmutableQuery, NonTrackingQuery, Query};

impl Fetch<'_> for Skip {
    type Item = Skip;

    #[inline]
    fn dangling() -> Self {
        Skip
    }

    #[inline]
    unsafe fn skip_chunk(&self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn skip_item(&self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn get_item(&mut self, _: usize) -> Skip {
        Skip
    }
}

unsafe impl Query for Skip {
    type Fetch = Skip;

    #[inline]
    fn mutates() -> bool {
        false
    }

    #[inline]
    fn access(_ty: TypeId) -> Access {
        Access::None
    }

    #[inline]
    fn allowed_with<Q: Query>() -> bool {
        true
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
    unsafe fn fetch(_: &Archetype, _: u64, _epoch: u64) -> Option<Skip> {
        Some(Skip)
    }
}

unsafe impl ImmutableQuery for Skip {}
unsafe impl NonTrackingQuery for Skip {}
