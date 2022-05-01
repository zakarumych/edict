use core::any::TypeId;

use crate::{archetype::Archetype, epoch::Epoch, proof::Skip};

use super::{Access, Fetch, ImmutableQuery, NonTrackingQuery, Query};

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
    fn skip_archetype(_: &Archetype, _: Epoch) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(_: &Archetype, _: Epoch, _epoch: Epoch) -> Option<Skip> {
        Some(Skip)
    }
}

unsafe impl ImmutableQuery for Skip {}
unsafe impl NonTrackingQuery for Skip {}
