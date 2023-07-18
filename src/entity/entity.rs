use core::{fmt, marker::PhantomData, num::NonZeroU64};

use crate::{
    bundle::{Bundle, DynamicBundle, DynamicComponentBundle},
    component::Component,
    query::{DefaultQuery, IntoQuery, QueryItem},
    view::ViewOne,
    world::World,
    NoSuchEntity, ResultEntityError,
};

use super::{EntitySet, Location};

/// General entity reference.
pub trait Entity {
    /// Returns entity id which is the weakest reference to the entity.
    fn id(&self) -> EntityId;

    /// Returns entity location if it is alive.
    fn lookup(&self, entities: &EntitySet) -> Option<Location>;

    /// Checks if the entity is alive.
    #[inline(always)]
    fn is_alive(&self, entities: &EntitySet) -> bool {
        self.lookup(entities).is_some()
    }

    /// Returns entity with bound location if it is alive.
    #[inline(always)]
    fn entity_loc<'a>(&self, entities: &'a EntitySet) -> Option<EntityLoc<'a>> {
        self.lookup(entities).map(|loc| EntityLoc {
            id: self.id(),
            loc,
            world: PhantomData,
        })
    }

    /// Returns entity reference if it is alive.
    #[inline(always)]
    fn entity_ref<'a>(&self, world: &'a mut World) -> Option<EntityRef<'a>> {
        self.lookup(world.entities()).map(|loc| EntityRef {
            id: self.id(),
            loc,
            world,
        })
    }
}

/// Entity which must stay alive while the reference is alive.
/// Produced by queries that yield related entities.
pub trait AliveEntity: Entity {
    /// Returns entity location.
    fn locate(&self, entities: &EntitySet) -> Location;

    /// Returns entity with bound location.
    #[inline(always)]
    fn entity_loc<'a>(&self, entities: &'a EntitySet) -> EntityLoc<'a> {
        let loc = self.locate(entities);
        EntityLoc {
            id: self.id(),
            loc,
            world: PhantomData,
        }
    }

    /// Returns entity reference if it is alive.
    #[inline(always)]
    fn entity_ref<'a>(&self, world: &'a mut World) -> EntityRef<'a> {
        let loc = self.locate(world.entities());
        EntityRef {
            id: self.id(),
            loc,
            world,
        }
    }
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

impl PartialEq<EntityBound<'_>> for EntityId {
    #[inline(always)]
    fn eq(&self, other: &EntityBound<'_>) -> bool {
        *self == other.id
    }
}

impl PartialEq<EntityLoc<'_>> for EntityId {
    #[inline(always)]
    fn eq(&self, other: &EntityLoc<'_>) -> bool {
        *self == other.id
    }
}

impl PartialEq<EntityRef<'_>> for EntityId {
    #[inline(always)]
    fn eq(&self, other: &EntityRef<'_>) -> bool {
        *self == other.id
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EntityId").field(&self.id).finish()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl EntityId {
    const DANGLING: NonZeroU64 = super::allocator::END;

    /// Returns a dangling entity ID that no real entity can have.
    pub const fn dangling() -> Self {
        EntityId { id: Self::DANGLING }
    }

    #[inline(always)]
    pub(super) fn new(id: NonZeroU64) -> Self {
        EntityId { id }
    }

    #[inline(always)]
    pub(super) fn non_zero(&self) -> NonZeroU64 {
        self.id
    }

    /// Returns the raw bits of the entity ID.
    #[inline(always)]
    pub fn bits(&self) -> u64 {
        self.id.get()
    }

    /// Returns the entity ID from the raw bits.
    /// Returns none if the bits are zero.
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

impl PartialEq<EntityId> for EntityBound<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == *other
    }
}

impl PartialEq<EntityLoc<'_>> for EntityBound<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityLoc<'_>) -> bool {
        self.id == other.id
    }
}

impl PartialEq<EntityRef<'_>> for EntityBound<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityRef<'_>) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for EntityBound<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EntityBound").field(&self.id).finish()
    }
}

impl fmt::Display for EntityBound<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl EntityBound<'_> {
    /// Returns entity id.
    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
    }

    #[inline(always)]
    pub(crate) fn new(id: EntityId) -> Self {
        EntityBound {
            id,
            world: PhantomData,
        }
    }

    #[inline(always)]
    pub(crate) fn wrap_slice(ids: &[EntityId]) -> &[Self] {
        // Safety: `Self` is transparent wrapper over `EntityId`.
        unsafe { &*(ids as *const [EntityId] as *const [Self]) }
    }
}

impl<'a> Entity for EntityBound<'a> {
    #[inline(always)]
    fn id(&self) -> EntityId {
        self.id
    }

    #[inline(always)]
    fn lookup(&self, entities: &EntitySet) -> Option<Location> {
        Some(self.locate(entities))
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

impl PartialEq<EntityId> for EntityLoc<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == *other
    }
}

impl PartialEq<EntityBound<'_>> for EntityLoc<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityBound<'_>) -> bool {
        self.id == other.id
    }
}

impl PartialEq<EntityRef<'_>> for EntityLoc<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityRef<'_>) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for EntityLoc<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityLoc")
            .field("id", &self.id)
            .field("loc", &self.loc)
            .finish()
    }
}

impl fmt::Display for EntityLoc<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
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

    /// Returns entity id.
    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
    }
}

impl<'a> Entity for EntityLoc<'a> {
    #[inline(always)]
    fn id(&self) -> EntityId {
        self.id
    }

    #[inline(always)]
    fn lookup(&self, _entities: &EntitySet) -> Option<Location> {
        Some(self.loc)
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

impl PartialEq<EntityId> for EntityRef<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == *other
    }
}

impl PartialEq<EntityBound<'_>> for EntityRef<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityBound<'_>) -> bool {
        self.id == other.id
    }
}

impl PartialEq<EntityLoc<'_>> for EntityRef<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityLoc<'_>) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for EntityRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityRef")
            .field("id", &self.id)
            .field("loc", &self.loc)
            .finish()
    }
}

impl fmt::Display for EntityRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl<'a> EntityRef<'a> {
    /// Returns entity reference if it is alive.
    #[inline(always)]
    pub fn new(id: EntityId, world: &'a mut World) -> Result<Self, NoSuchEntity> {
        let loc = world.entities().get_location(id).ok_or(NoSuchEntity)?;
        Ok(EntityRef { id, loc, world })
    }

    #[inline(always)]
    pub(crate) unsafe fn from_parts(id: EntityId, loc: Location, world: &'a mut World) -> Self {
        debug_assert_eq!(world.entities().get_location(id), Some(loc));
        EntityRef { id, loc, world }
    }

    /// Queries components from specified entity.
    ///
    /// Returns query item.
    ///
    /// This method works only for default-constructed query types.
    ///
    /// Mutably borrows world for the duration of query item's lifetime,
    /// avoiding runtime borrow checks.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn get<'b, Q>(&'b mut self) -> Option<QueryItem<'b, Q>>
    where
        Q: DefaultQuery,
    {
        self.get_with(Q::default_query())
    }

    /// Queries components from specified entity.
    ///
    /// Returns query item.
    ///
    /// Mutably borrows world for the duration of query item's lifetime,
    /// avoiding runtime borrow checks.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn get_with<'b, Q>(&'b mut self, query: Q) -> Option<QueryItem<'b, Q::Query>>
    where
        Q: IntoQuery,
    {
        unsafe { self.get_with_unchecked(query) }
    }

    /// Queries components from specified entity.
    ///
    /// Returns query item.
    ///
    /// This method works only for default-constructed query types.
    ///
    /// # Safety
    ///
    /// Caller must guarantee to not create invalid aliasing of component
    /// references.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub unsafe fn get_unchecked<'b, Q>(&'b self) -> Option<QueryItem<'b, Q>>
    where
        Q: DefaultQuery,
    {
        unsafe { self.get_with_unchecked(Q::default_query()) }
    }

    /// Queries components from specified entity.
    ///
    /// Returns query item.
    ///
    /// # Safety
    ///
    /// Caller must guarantee to not create invalid aliasing of component
    /// references.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub unsafe fn get_with_unchecked<'b, Q>(&'b self, query: Q) -> Option<QueryItem<'b, Q::Query>>
    where
        Q: IntoQuery,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe {
            self.world
                .get_with_unchecked(loc, query)
                .assume_entity_exists()
        }
    }

    /// Queries components from specified entity.
    ///
    /// Returns a wrapper from which query item can be fetched.
    ///
    /// The wrapper holds borrow locks for entity's archetype and releases them on drop.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn view_one<'b, Q>(&'b self) -> ViewOne<'b, Q>
    where
        Q: DefaultQuery,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        self.world.view_one::<Q>(loc)
    }

    /// Queries components from specified entity.
    /// This method accepts query instance to support stateful queries.
    ///
    /// This method works only for stateless query types.
    /// Returned wrapper holds borrow locks for entity's archetype and releases them on drop.
    ///
    /// # Panics
    ///
    /// This method may panic if entity of another world is used.
    #[inline(always)]
    pub fn view_one_with<'b, Q>(&'b self, query: Q) -> ViewOne<'b, (Q,)>
    where
        Q: IntoQuery,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        self.world.view_one_with::<Q>(loc, query)
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    pub fn get_cloned<T>(&self) -> Option<T>
    where
        T: Clone + Sync + 'static,
    {
        self.view_one::<&T>().map(Clone::clone)
    }

    /// Insert a component to the entity.
    #[inline(always)]
    pub fn insert<T>(&mut self, component: T)
    where
        T: Component,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe { self.world.insert(loc, component).unwrap_unchecked() }
    }

    /// Attempts to inserts component to the entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    ///
    /// let mut entity = world.spawn(());
    ///
    /// assert_eq!(!entity.has_component::<u32>());
    /// entity.insert_external(42u32);
    /// assert!(entity.has_component::<u32>());
    /// ```
    #[inline(always)]
    pub fn insert_external<T>(&mut self, component: T)
    where
        T: 'static,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe {
            self.world
                .insert_external(loc, component)
                .unwrap_unchecked()
        }
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let mut entity = world.spawn(());
    /// assert_eq!(!entity.has_component::<ExampleComponent>());
    /// entity.insert_bundle((ExampleComponent,));
    /// assert_eq!(entity.has_component::<ExampleComponent>(entity));
    /// ```
    #[inline(always)]
    pub fn insert_bundle<B>(&mut self, bundle: B)
    where
        B: DynamicComponentBundle,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe { self.world.insert_bundle(loc, bundle).unwrap_unchecked() }
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// let mut entity = world.spawn(());
    ///
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// assert!(!entity.has_component::<u32>());
    ///
    /// entity.insert_external_bundle((ExampleComponent, 42u32));
    ///
    /// assert!(entity.has_component::<ExampleComponent>());
    /// assert!(entity.has_component::<u32>());
    /// ```
    #[inline(always)]
    pub fn insert_external_bundle<B>(&mut self, bundle: B)
    where
        B: DynamicBundle,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe {
            self.world
                .insert_external_bundle(loc, bundle)
                .unwrap_unchecked()
        }
    }

    /// Removes a component from the entity.
    /// Returns the component if it was present.
    #[inline(always)]
    pub fn remove<T>(&mut self) -> Option<T>
    where
        T: 'static,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe { self.world.remove(loc).unwrap_unchecked() }
    }

    /// Drops a component from the referenced entity.
    #[inline(always)]
    pub fn drop<T>(&mut self)
    where
        T: 'static,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe { self.world.drop::<T>(loc).unwrap_unchecked() }
    }

    /// Drops entity's components that are found in the specified bundle.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// Unlike other methods that use `Bundle` trait, this method does not require
    /// all components from bundle to be registered in the world.
    /// Entity can't have components that are not registered in the world,
    /// so no need to drop them.
    ///
    /// For this reason there's no separate method that uses `ComponentBundle` trait.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    ///
    /// struct OtherComponent;
    ///
    /// let mut world = World::new();
    /// let mut entity = world.spawn((ExampleComponent,));
    ///
    /// assert!(entity.has_component::<ExampleComponent>());
    ///
    /// entity.drop_bundle::<(ExampleComponent, OtherComponent)>();
    ///
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline(always)]
    pub fn drop_bundle<B>(&mut self)
    where
        B: Bundle,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe { self.world.drop_bundle::<B>(loc).unwrap_unchecked() }
    }

    /// Despawns the referenced entity.
    #[inline(always)]
    pub fn despawn(self) {
        unsafe { self.world.despawn_ref(self.id, self.loc) }
    }

    /// Checks if entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn has_component<T: 'static>(&self) -> bool {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        self.world.has_component::<T>(loc)
    }

    /// Splits entity reference into two parts: entity with location and world.
    /// This allows using `World` to access other entities,
    /// without loosing entity reference entirely.
    pub fn split(self) -> (EntityLoc<'a>, &'a World) {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        (loc, self.world)
    }

    /// Returns entity id.
    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
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
