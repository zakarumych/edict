use core::{any::TypeId, marker::PhantomData};

use crate::{archetype::Archetype, entity::EntityId};

use super::{Access, Fetch, ImmutablePhantomQuery, IntoQuery, PhantomQuery};

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
    unsafe fn get_item(&mut self, idx: usize) -> EntityId {
        *self.entities.get_unchecked(idx)
    }
}

/// Queries entity ids.
#[derive(Clone, Copy, Debug, Default)]
pub struct Entities;

impl IntoQuery for Entities {
    type Query = PhantomData<fn() -> Self>;
}

unsafe impl PhantomQuery for Entities {
    type Fetch<'a> = EntitiesFetch<'a>;
    type Item<'a> = EntityId;

    #[inline]
    fn access(_ty: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn visit_archetype(_archetype: &Archetype) -> bool {
        true
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, _f: &dyn Fn(TypeId, Access)) {}

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

unsafe impl ImmutablePhantomQuery for Entities {}
