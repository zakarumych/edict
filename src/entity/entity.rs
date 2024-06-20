use core::{any::TypeId, fmt, marker::PhantomData, num::NonZeroU64};

use crate::{
    bundle::{Bundle, DynamicBundle, DynamicComponentBundle},
    component::Component,
    flow::IntoEntityFlow,
    // flow::FlowEntityFn,
    query::{DefaultQuery, ImmutableQuery, IntoQuery, QueryItem},
    view::ViewOne,
    world::{World, WorldLocal},
    NoSuchEntity,
    ResultEntityError,
};

use super::{EntitySet, Location};

/// General entity reference.
pub trait Entity: Copy {
    /// Returns entity id which is the weakest reference to the entity.
    fn id(&self) -> EntityId;

    /// Returns entity location if it is alive.
    fn lookup(&self, entities: &EntitySet) -> Option<Location>;

    /// Checks if the entity is alive.
    fn is_alive(&self, entities: &EntitySet) -> bool;

    /// Returns entity with bound location if it is alive.
    fn entity_loc<'a>(&self, entities: &'a EntitySet) -> Option<EntityLoc<'a>>;

    /// Returns entity reference if it is alive.
    fn entity_ref<'a>(&self, world: &'a mut World) -> Option<EntityRef<'a>>;
}

/// Entity which must stay alive while the reference is alive.
/// Produced by queries that yield related entities.
pub trait AliveEntity: Entity {
    /// Returns entity location.
    #[inline(always)]
    fn locate(&self, entities: &EntitySet) -> Location {
        entities
            .get_location(self.id())
            .expect("Entity is not alive")
    }

    /// Returns entity with bound location.
    #[inline(always)]
    fn entity_loc<'a>(&self, entities: &'a EntitySet) -> EntityLoc<'a> {
        EntityLoc::from_alive(*self, entities)
    }

    /// Returns entity reference if it is alive.
    #[inline(always)]
    fn entity_ref<'a>(&self, _world: &'a mut World) -> EntityRef<'a> {
        unreachable!()
    }
}

/// Entity ID.
/// The ID is unique within the world.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct EntityId {
    id: NonZeroU64,
}

#[cfg(feature = "serde")]
impl serde::ser::Serialize for EntityId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.id.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::de::Deserialize<'de> for EntityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let id = NonZeroU64::deserialize(deserializer)?;
        Ok(EntityId { id })
    }
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
    pub(super) const fn new(id: NonZeroU64) -> Self {
        EntityId { id }
    }

    #[inline(always)]
    pub(super) const fn non_zero(&self) -> NonZeroU64 {
        self.id
    }

    /// Returns the raw bits of the entity ID.
    #[inline(always)]
    pub const fn bits(&self) -> u64 {
        self.id.get()
    }

    /// Returns the entity ID from the raw bits.
    /// Returns none if the bits are zero.
    #[inline(always)]
    pub const fn from_bits(bits: u64) -> Option<Self> {
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

    #[inline(always)]
    fn is_alive(&self, entities: &EntitySet) -> bool {
        entities.is_alive(*self)
    }

    #[inline(always)]
    fn entity_loc<'a>(&self, entities: &'a EntitySet) -> Option<EntityLoc<'a>> {
        Some(EntityLoc::from_parts(*self, entities.get_location(*self)?))
    }

    /// Returns entity reference if it is alive.
    #[inline(always)]
    fn entity_ref<'a>(&self, world: &'a mut World) -> Option<EntityRef<'a>> {
        EntityRef::new(*self, world).ok()
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

    #[inline(always)]
    fn is_alive(&self, _entities: &EntitySet) -> bool {
        true
    }

    #[inline(always)]
    fn entity_loc<'b>(&self, entities: &'b EntitySet) -> Option<EntityLoc<'b>> {
        Some(EntityLoc::from_alive(*self, entities))
    }

    /// Returns entity reference if it is alive.
    #[inline(always)]
    fn entity_ref<'b>(&self, _world: &'b mut World) -> Option<EntityRef<'b>> {
        unreachable!()
    }
}

impl<'a> AliveEntity for EntityBound<'a> {
    /// Returns entity location.
    #[inline(always)]
    fn locate(&self, entities: &EntitySet) -> Location {
        // If this panics it is probably a bug in edict
        // or entity belongs to another world.
        entities.get_location(self.id()).expect("Bound entity")
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
    pub(crate) fn from_parts(id: EntityId, loc: Location) -> Self {
        EntityLoc {
            id,
            loc,
            world: PhantomData,
        }
    }

    #[inline(always)]
    pub(crate) fn from_alive(entity: impl AliveEntity, entities: &EntitySet) -> Self {
        EntityLoc {
            id: entity.id(),
            loc: entity.locate(entities),
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

    #[inline(always)]
    fn is_alive(&self, _entities: &EntitySet) -> bool {
        true
    }

    #[inline(always)]
    fn entity_loc<'b>(&self, _entities: &'b EntitySet) -> Option<EntityLoc<'b>> {
        Some(EntityLoc::from_parts(self.id, self.loc))
    }

    /// Returns entity reference if it is alive.
    #[inline(always)]
    fn entity_ref<'b>(&self, _world: &'b mut World) -> Option<EntityRef<'b>> {
        unreachable!();
    }
}

impl<'a> AliveEntity for EntityLoc<'a> {
    #[inline(always)]
    fn locate(&self, _entities: &EntitySet) -> Location {
        self.loc
    }

    #[inline(always)]
    fn entity_loc<'b>(&self, _entities: &'b EntitySet) -> EntityLoc<'b> {
        EntityLoc::from_parts(self.id, self.loc)
    }

    /// Returns entity reference if it is alive.
    #[inline(always)]
    fn entity_ref<'b>(&self, _world: &'b mut World) -> EntityRef<'b> {
        unreachable!()
    }
}

/// EntityRef is a mutable reference to an entity.
/// It borrows the world mutably, so it can be used to modify the entity freely.
pub struct EntityRef<'a> {
    id: EntityId,
    loc: Location,
    world: &'a mut WorldLocal,
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
        world.maintenance();
        let loc = world.entities().get_location(id).ok_or(NoSuchEntity)?;
        Ok(EntityRef {
            id,
            loc,
            world: world.local(),
        })
    }

    #[inline(always)]
    pub(crate) fn from_parts(id: EntityId, loc: Location, world: &'a mut World) -> Self {
        debug_assert_eq!(world.entities().get_location(id), Some(loc));
        EntityRef {
            id,
            loc,
            world: world.local(),
        }
    }

    /// Returns entity id.
    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Queries components from the entity.
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
    pub fn get<'b, Q>(&'b self) -> Option<QueryItem<'b, Q>>
    where
        Q: DefaultQuery,
        Q::Query: ImmutableQuery,
    {
        self.get_with(Q::default_query())
    }

    /// Queries components from the entity.
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
    pub fn get_mut<'b, Q>(&'b mut self) -> Option<QueryItem<'b, Q>>
    where
        Q: DefaultQuery,
    {
        self.get_with_mut(Q::default_query())
    }

    /// Queries components from the entity.
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
    pub fn get_with<'b, Q>(&'b self, query: Q) -> Option<QueryItem<'b, Q::Query>>
    where
        Q: IntoQuery,
        Q::Query: ImmutableQuery,
    {
        unsafe { self.get_with_unchecked(query) }
    }

    /// Queries components from the entity.
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
    pub fn get_with_mut<'b, Q>(&'b mut self, query: Q) -> Option<QueryItem<'b, Q::Query>>
    where
        Q: IntoQuery,
    {
        unsafe { self.get_with_unchecked(query) }
    }

    /// Queries components from the entity.
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

    /// Queries components from the entity.
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

    /// Queries components from the entity.
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

    /// Queries components from the entity.
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

    /// Queries components from the entity.
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
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// This may consume entity reference because insertion may execute a hook
    /// that will despawn the entity.
    #[inline(always)]
    pub fn insert<T>(self, component: T) -> Option<Self>
    where
        T: Component,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe {
            self.world.insert(loc, component).unwrap_unchecked();
        }

        let loc = self.world.entities().get_location(self.id)?;
        Some(EntityRef {
            id: self.id,
            loc,
            world: self.world,
        })
    }

    /// Insert external a component to the entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// This consumes entity because insertion may execute a hook
    /// which may invalidate the entity reference and even despawn the entity.
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
    /// assert!(!entity.has_component::<u32>());
    ///
    /// let id = entity.id();
    /// entity.insert_external(42u32);
    ///
    /// assert!(world.try_has_component::<u32>(id).unwrap());
    /// ```
    #[inline(always)]
    pub fn insert_external<T>(self, component: T) -> Option<Self>
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
                .unwrap_unchecked();
        }

        let loc = self.world.entities().get_location(self.id)?;
        Some(EntityRef {
            id: self.id,
            loc,
            world: self.world,
        })
    }

    /// Checks if entity has a component of specified type.
    /// Inserts component if it is missing.
    /// Returns a mutable reference to the component.
    ///
    /// Unlike `insert` this may neber cause hooks to be executed
    /// so reference is guaranteed to be valid.
    #[inline(always)]
    pub fn with<T>(&mut self, f: impl FnOnce() -> T) -> &mut T
    where
        T: Component,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        self.loc = unsafe { self.world.with(loc, f).unwrap_unchecked() }.loc;

        unsafe {
            self.world
                .archetypes_mut()
                .get_unchecked_mut(self.loc.arch as usize)
                .get_mut_nobump(self.loc.idx)
        }
    }

    /// Attempts to inserts component to the entity.
    ///
    /// If entity already had component of that type,
    /// closure is not called.
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
    /// assert!(!entity.has_component::<u32>());
    /// entity.with_external(|| 42u32);
    /// assert!(entity.has_component::<u32>());
    /// ```
    #[inline(always)]
    pub fn with_external<T>(&mut self, component: impl FnOnce() -> T) -> &mut T
    where
        T: 'static,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        self.loc = unsafe { self.world.with_external(loc, component).unwrap_unchecked() }.loc;

        unsafe {
            self.world
                .archetypes_mut()
                .get_unchecked_mut(self.loc.arch as usize)
                .get_mut_nobump(self.loc.idx)
        }
    }

    /// Inserts bundle of components to the entity.
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
    /// assert!(!entity.has_component::<ExampleComponent>());
    ///
    /// let id = entity.id();
    /// entity.insert_bundle((ExampleComponent,));
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(id).unwrap());
    /// ```
    #[inline(always)]
    pub fn insert_bundle<B>(self, bundle: B) -> Option<Self>
    where
        B: DynamicComponentBundle,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        unsafe {
            self.world.insert_bundle(loc, bundle).unwrap_unchecked();
        }

        let loc = self.world.entities().get_location(self.id)?;
        Some(EntityRef {
            id: self.id,
            loc,
            world: self.world,
        })
    }

    /// Inserts bundle of components to the entity.
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
    /// let id = entity.id();
    /// entity.insert_external_bundle((ExampleComponent, 42u32));
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(id).unwrap());
    /// assert!(world.try_has_component::<u32>(id).unwrap());
    /// ```
    #[inline(always)]
    pub fn insert_external_bundle<B>(self, bundle: B) -> Option<Self>
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
                .unwrap_unchecked();
        }

        let loc = self.world.entities().get_location(self.id)?;
        Some(EntityRef {
            id: self.id,
            loc,
            world: self.world,
        })
    }

    /// Inserts bundle of components to the entity.
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
    ///
    /// assert!(!entity.has_component::<ExampleComponent>());
    ///
    /// let id = entity.id();
    /// entity.insert_bundle((ExampleComponent,));
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(id).unwrap());
    /// ```
    #[inline(always)]
    pub fn with_bundle<B>(&mut self, bundle: B)
    where
        B: DynamicComponentBundle,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        self.loc = unsafe { self.world.with_bundle(loc, bundle).unwrap_unchecked() }.loc;
    }

    /// Inserts bundle of components to the entity.
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
    /// let id = entity.id();
    /// entity.with_external_bundle((ExampleComponent, 42u32));
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(id).unwrap());
    /// assert!(world.try_has_component::<u32>(id).unwrap());
    /// ```
    #[inline(always)]
    pub fn with_external_bundle<B>(&mut self, bundle: B)
    where
        B: DynamicBundle,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };
        self.loc = unsafe {
            self.world
                .with_external_bundle(loc, bundle)
                .unwrap_unchecked()
        }
        .loc;
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
        let (c, e) = unsafe { self.world.remove(loc).unwrap_unchecked() };
        self.loc = e.loc;
        c
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop<T>(self) -> Option<Self>
    where
        T: 'static,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };

        unsafe {
            self.world.drop::<T>(loc).unwrap_unchecked();
        }

        let loc = self.world.entities().get_location(self.id)?;
        Some(EntityRef {
            id: self.id,
            loc,
            world: self.world,
        })
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop_erased(self, ty: TypeId) -> Option<Self> {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };

        unsafe {
            self.world.drop_erased(loc, ty).unwrap_unchecked();
        }

        let loc = self.world.entities().get_location(self.id)?;
        Some(EntityRef {
            id: self.id,
            loc,
            world: self.world,
        })
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
    /// let id = entity.id();
    /// entity.drop_bundle::<(ExampleComponent, OtherComponent)>();
    ///
    /// assert!(!world.try_has_component::<ExampleComponent>(id).unwrap());
    /// ```
    #[inline(always)]
    pub fn drop_bundle<B>(self) -> Option<Self>
    where
        B: Bundle,
    {
        let loc = EntityLoc {
            id: self.id,
            loc: self.loc,
            world: PhantomData,
        };

        unsafe {
            self.world.drop_bundle::<B>(loc).unwrap_unchecked();
        }

        let loc = self.world.entities().get_location(self.id)?;
        Some(EntityRef {
            id: self.id,
            loc,
            world: self.world,
        })
    }

    /// Despawns the entity.
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

    /// Spawns a new flow for the entity.
    pub fn spawn_flow<F>(&mut self, f: F)
    where
        F: IntoEntityFlow,
    {
        let id = self.id;
        self.world.spawn_flow_for(id, f);
    }
}
