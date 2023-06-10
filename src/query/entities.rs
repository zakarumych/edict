use core::{any::TypeId, marker::PhantomData};

use crate::{
    archetype::Archetype,
    entity::{Entity, EntityId, Located, Location},
};

use super::{Access, Fetch, ImmutablePhantomQuery, PhantomQuery};

/// [`Fetch`] type for the [`Entities`] query.
pub struct EntitiesFetch<'a> {
    archetype: u32,
    entities: &'a [EntityId],
}

unsafe impl<'a> Fetch<'a> for EntitiesFetch<'a> {
    type Item = Entity<Located<'a>>;

    #[inline]
    fn dangling() -> Self {
        EntitiesFetch {
            archetype: u32::MAX,
            entities: &[],
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> Entity<Located<'a>> {
        let id = *self.entities.get_unchecked(idx);
        Entity::new_located(id, Location::new(self.archetype, idx))
    }
}

/// Queries entity ids.
#[derive(Clone, Copy, Debug, Default)]
pub struct Entities;

/// Query type for the [`Entities`] phantom query.
pub type EntitiesQuery = PhantomData<fn() -> Entities>;

impl Entities {
    /// Creates a new [`Entities`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
}

unsafe impl PhantomQuery for Entities {
    type Fetch<'a> = EntitiesFetch<'a>;
    type Item<'a> = Entity<Located<'a>>;

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
        archetype_idx: u32,
        archetype: &'a Archetype,
        _epoch: crate::epoch::EpochId,
    ) -> EntitiesFetch<'a> {
        EntitiesFetch {
            archetype: archetype_idx,
            entities: archetype.entities(),
        }
    }

    #[inline]
    fn reserved_entity_item<'a>(id: EntityId, idx: u32) -> Option<Entity<Located<'a>>>
    where
        EntityId: 'a,
    {
        Some(Entity::new_located(id, Location::reserved(idx)))
    }
}

unsafe impl ImmutablePhantomQuery for Entities {}
