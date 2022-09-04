use core::{any::TypeId, marker::PhantomData};

use crate::{archetype::Archetype, entity::EntityId};

use super::{Access, Fetch, IntoQuery, PhantomQuery, PhantomQueryFetch};

/// [`Fetch`] type for the [`Entities`] query.
pub struct EntitiesFetch<'a> {
    entities: &'a [EntityId],
}

unsafe impl<'a> Fetch<'a> for EntitiesFetch<'a> {
    type Item = EntityId;

    #[inline]
    fn dangling() -> Self {
        EntitiesFetch { entities: &[] }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _chunk_idx: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _chunk_idx: usize) {}

    #[inline]
    unsafe fn skip_item(&mut self, _idx: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> EntityId {
        *self.entities.get_unchecked(idx)
    }
}

/// Queries entity ids.
#[derive(Default)]
pub struct Entities;

impl<'a> PhantomQueryFetch<'a> for Entities {
    type Fetch = EntitiesFetch<'a>;
    type Item = EntityId;
}

impl IntoQuery for Entities {
    type Query = PhantomData<fn() -> Self>;
}

impl PhantomQuery for Entities {
    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn skip_archetype(_archetype: &Archetype) -> bool {
        false
    }

    #[inline]
    unsafe fn fetch<'a>(
        archetype: &'a Archetype,
        _epoch: crate::epoch::EpochId,
    ) -> EntitiesFetch<'a> {
        EntitiesFetch {
            entities: archetype.entities(),
        }
    }
}
