use core::{fmt, marker::PhantomData, num::NonZeroU64};

use crate::{component::Component, world::World};

use super::{EntitySet, Location};

/// Error when an entity is not found in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoSuchEntity;

impl fmt::Display for NoSuchEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Specified entity is not found")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NoSuchEntity {}

/// General entity reference.
pub trait Entity {
    /// Returns entity id which is the weakest reference to the entity.
    fn id(&self) -> EntityId;

    /// Returns entity location if it is alive.
    fn lookup(&self, entities: &EntitySet) -> Option<Location>;
}

/// Entity which must stay alive while the reference is alive.
/// Produced by queries that yield related entities.
pub trait AliveEntity: Entity {
    /// Returns entity location.
    fn locate(&self, entities: &EntitySet) -> Location;
}

/// Entity which is guaranteed to be alive
/// and has known location.
pub trait LocatedEntity: AliveEntity {
    /// Returns entity location.
    fn location(&self) -> Location;
}

/// Entity ID.
/// The ID is unique within the world.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct EntityId {
    id: NonZeroU64,
}

impl EntityId {
    #[inline(always)]
    pub(super) fn non_zero(&self) -> NonZeroU64 {
        self.id
    }

    #[inline(always)]
    pub fn bits(&self) -> u64 {
        self.id.get()
    }

    #[inline(always)]
    pub fn from_bits(bits: u64) -> Option<Self> {
        match NonZeroU64::new(bits) {
            Some(id) => Some(EntityId { id }),
            None => None,
        }
    }
}

impl Entity for EntityId {
    #[inline(always)]
    fn id(&self) -> EntityId {
        *self
    }

    #[inline(always)]
    fn lookup(&self, entities: &EntitySet) -> Option<Location> {
        entities.get_location(*self)
    }
}

/// Entity reference that is guaranteed to be alive.
/// The value is bound to the world borrow
/// that prevents the entity from being removed.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct EntityBound<'a> {
    id: EntityId,
    world: PhantomData<&'a World>,
}

impl EntityBound<'_> {
    #[inline(always)]
    pub fn new(id: EntityId) -> Self {
        EntityBound {
            id,
            world: PhantomData,
        }
    }
}

impl<'a> Entity for EntityBound<'a> {
    #[inline(always)]
    fn id(&self) -> EntityId {
        self.id
    }

    #[inline(always)]
    fn lookup(&self, entities: &EntitySet) -> Option<Location> {
        Ok(self.locate(entities))
    }
}

impl<'a> AliveEntity for EntityBound<'a> {
    #[inline(always)]
    fn locate(&self, entities: &EntitySet) -> Location {
        entities.get_location(self.id).expect("Entity is not alive")
    }
}

/// Entity reference that is guaranteed to be alive.
/// The value is bound to the world borrow
/// that prevents the entity from being removed.
/// The entity location is known.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityLoc<'a> {
    id: EntityId,
    loc: Location,
    world: PhantomData<&'a World>,
}

impl EntityLoc<'_> {
    #[inline(always)]
    pub(crate) fn new(id: EntityId, loc: Location) -> Self {
        EntityLoc {
            id,
            loc,
            world: PhantomData,
        }
    }
}

impl<'a> Entity for EntityLoc<'a> {
    #[inline(always)]
    fn id(&self) -> EntityId {
        self.id
    }

    #[inline(always)]
    fn lookup(&self, _entities: &EntitySet) -> Option<Location> {
        Ok(self.loc)
    }
}

impl<'a> AliveEntity for EntityLoc<'a> {
    #[inline(always)]
    fn locate(&self, _entities: &EntitySet) -> Location {
        self.loc
    }
}

impl<'a> LocatedEntity for EntityLoc<'a> {
    #[inline(always)]
    fn location(&self) -> Location {
        self.loc
    }
}

/// EntityRef is a mutable reference to an entity.
/// It borrows the world mutably, so it can be used to modify the entity freely.
pub struct EntityRef<'a> {
    id: EntityId,
    loc: Location,
    world: &'a mut World,
}

impl<'a> EntityRef<'a> {
    #[inline(always)]
    pub(crate) fn new(id: EntityId, loc: Location, world: &'a mut World) -> Self {
        EntityRef {
            id,
            loc,
            world: PhantomData,
        }
    }

    #[inline(always)]
    fn loc(&self) -> EntityLoc<'_> {
        EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        }
    }

    #[inline(always)]
    pub fn get(id: EntityId, world: &'a mut World) -> Result<Self, NoSuchEntity> {
        let loc = world.entity_set().get_location(id).ok_or(NoSuchEntity)?;
        Ok(EntityRef { id, loc, world })
    }

    /// Insert a component to the entity.
    #[inline(always)]
    pub fn insert<T>(&mut self, component: T)
    where
        T: Component,
    {
        unsafe { self.world.insert(self.loc(), component).unwrap_unchecked() }
    }

    /// Removes a component from the entity.
    /// Returns the component if it was present.
    #[inline(always)]
    pub fn remove<T>(&mut self) -> Option<T>
    where
        T: 'static,
    {
        unsafe { self.world.remove(self.loc()).unwrap_unchecked() }
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop<T>(&mut self)
    where
        T: 'static,
    {
        unsafe { self.world.drop(self.loc()).unwrap_unchecked() }
    }

    #[inline(always)]
    pub fn despawn(self) {
        unsafe { self.world.despawn(self.loc()).unwrap_unchecked() }
    }
}

impl<'a> Entity for EntityRef<'a> {
    #[inline(always)]
    fn id(&self) -> EntityId {
        self.id
    }

    #[inline(always)]
    fn lookup(&self, _entities: &EntitySet) -> Option<Location> {
        Some(self.loc)
    }
}

impl<'a> AliveEntity for EntityRef<'a> {
    #[inline(always)]
    fn locate(&self, _entities: &EntitySet) -> Location {
        self.loc
    }
}

impl<'a> LocatedEntity for EntityRef<'a> {
    #[inline(always)]
    fn location(&self) -> Location {
        self.loc
    }
}
