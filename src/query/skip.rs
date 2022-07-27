use core::any::TypeId;

use crate::{archetype::Archetype, proof::Skip};

use super::{fetch::Fetch, phantom::PhantomQuery, Access, ImmutableQuery, Query};

unsafe impl Fetch<'_> for Skip {
    type Item = Skip;

    #[inline]
    fn dangling() -> Self {
        Skip
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn skip_item(&mut self, _: usize) -> bool {
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
    fn access(&self, _ty: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn conflicts<Q>(&self, _: &Q) -> bool
    where
        Q: Query,
    {
        false
    }

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn skip_archetype(&self, _: &Archetype) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(&mut self, _: &Archetype, _epoch: u64) -> Skip {
        Skip
    }
}

unsafe impl PhantomQuery for Skip {
    type Fetch = Skip;

    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn conflicts<Q>(_: &Q) -> bool
    where
        Q: Query,
    {
        false
    }

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn skip_archetype(_: &Archetype) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch(_: &Archetype, _epoch: u64) -> Skip {
        Skip
    }
}

unsafe impl ImmutableQuery for Skip {}
