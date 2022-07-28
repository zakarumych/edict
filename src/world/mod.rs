//! Self-contained ECS [`World`].

use core::{
    any::{type_name, TypeId},
    fmt,
    hash::Hash,
    iter::FromIterator,
    iter::FusedIterator,
    marker::PhantomData,
};

use alloc::vec::Vec;

use crate::{
    action::ActionEncoder,
    archetype::{chunk_idx, Archetype},
    bundle::{Bundle, DynamicBundle},
    component::{Component, ComponentRegistry},
    entity::{Entities, EntityId},
    query::{
        debug_assert_immutable_query, filter_relation_to, Fetch, FilterRelationTo,
        ImmutablePhantomQuery, ImmutableQuery, PhantomQuery, PhantomQueryItem, Query, QueryMut,
        QueryRef, QueryRelation, QueryRelationTo, With, Without,
    },
};

#[cfg(feature = "rc")]
use crate::{entity::Entity, proof::Proof};

#[cfg(feature = "relation")]
use crate::relation::Relation;

use self::edges::Edges;
pub use self::{builder::WorldBuilder, meta::EntityMeta};

// mod archetypes;
mod builder;
mod edges;
mod meta;

/// Limits on reserving of space for entities and components
/// in archetypes when `spawn_batch` is used.
const MAX_SPAWN_RESERVE: usize = 1024;

fn spawn_reserve(iter: &impl Iterator, archetype: &mut Archetype) {
    let (lower, upper) = iter.size_hint();
    let additional = match (lower, upper) {
        (lower, None) => lower,
        (lower, Some(upper)) => {
            // Iterator is consumed in full, so reserve at least `lower`.
            lower.max(upper.min(MAX_SPAWN_RESERVE))
        }
    };
    archetype.reserve(additional);
}

/// Container for entities with any sets of components.
///
/// Entities can be spawned in the `World` with handle `Entity` returned,
/// that can be used later to access that entity.
///
/// `Entity` handle can be downgraded to `EntityId`.
///
/// Entity would be despawned after last `Entity` is dropped.
///
/// Entity's set of components may be modified in any way.
///
/// Entities can be fetched directly, using `Entity` or `EntityId`
/// with different guarantees and requirements.
///
/// Entities can be efficiently queried from `World` to iterate over all entities
/// that match query requirements.
///
/// Internally `World` manages entities generations,
/// maps entity to location of components in archetypes,
/// moves components of entities between archetypes,
/// spawns and despawns entities.
#[allow(missing_debug_implementations)]
pub struct World {
    /// Global epoch counter of the World.
    /// Incremented on each mutable query.
    epoch: u64,

    /// Collection of entities with their locations.
    entities: Entities,

    /// Archetypes of entities in the world.
    archetypes: Vec<Archetype>,

    edges: Edges,

    registry: ComponentRegistry,

    /// Array of indices to drop.
    #[cfg(feature = "rc")]
    drop_queue: Vec<u32>,

    /// Internal action encoder.
    /// This encoder is used to record commands from component hooks.
    /// Commands are immediately executed at the end of the mutating call.
    cached_encoder: Option<ActionEncoder>,
}

impl Default for World {
    fn default() -> Self {
        World::new()
    }
}

macro_rules! with_encoder {
    ($world:ident, $encoder:ident => $expr:expr) => {{
        let mut $encoder = $world
            .cached_encoder
            .take()
            .unwrap_or_else(ActionEncoder::new);
        let result = $expr;
        ActionEncoder::execute(&mut $encoder, $world);
        $world.cached_encoder = Some($encoder);
        result
    }};
}

impl World {
    /// Returns new instance of [`WorldBuilder`]
    pub const fn builder() -> WorldBuilder {
        WorldBuilder::new()
    }

    /// Returns new instance of [`World`].
    ///
    /// Created [`World`] instance contains no entities.
    ///
    /// Internal caches that make operations faster are empty.
    /// This can make a small spike in latency
    /// as each cache entry would be calculated on first use of each key.
    #[inline]
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Spawns new entity in this world with provided bundle of components.
    /// World keeps ownership of the spawned entity and entity id is returned.
    #[inline]
    pub fn spawn<B>(&mut self, bundle: B) -> EntityId
    where
        B: DynamicBundle,
    {
        if !bundle.valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let entity = self.entities.spawn();

        let archetype_idx = self
            .edges
            .spawn(&mut self.registry, &mut self.archetypes, &bundle);

        self.epoch += 1;
        let idx = self.archetypes[archetype_idx as usize].spawn(entity, bundle, self.epoch);
        self.entities.set_location(entity.idx(), archetype_idx, idx);
        entity
    }

    /// Spawns new entity in this world with provided bundle of components.
    /// Returns owning reference to the entity.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn spawn_owning<B>(&mut self, bundle: B) -> Entity
    where
        B: DynamicBundle,
    {
        if !bundle.valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let entity = self.entities.spawn_owning();

        let archetype_idx = self
            .edges
            .spawn(&mut self.registry, &mut self.archetypes, &bundle);

        self.epoch += 1;
        let idx = self.archetypes[archetype_idx as usize].spawn(*entity, bundle, self.epoch);
        self.entities.set_location(entity.idx(), archetype_idx, idx);

        entity
    }

    /// Returns an iterator which spawns and yield entities
    /// using bundles returnd from provided iterator.
    ///
    /// When bundles iterator returns `None`, returned iterator returns `None` too.
    ///
    /// If bundles iterator is fused, returned iterator is fused too.
    /// If bundles iterator is double-ended, returned iterator is double-ended too.
    /// If bundles iterator has exact size, returned iterator has exact size too.
    ///
    /// Skipping items on returned iterator will not cause them to be spawned
    /// and same number of items will be skipped on bundles iterator.
    ///
    /// Returned iterator attempts to optimize storage allocation for entities
    /// if consumed with functions like `fold`, `rfold`, `for_each` or `collect`.
    ///
    /// When returned iterator is dropped, no more entities will be spawned
    /// even if bundles iterator has items left.
    #[inline]
    pub fn spawn_batch<B, I>(&mut self, bundles: I) -> SpawnBatch<'_, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let archetype_idx = self.edges.insert_bundle(
            &mut self.registry,
            &mut self.archetypes,
            0,
            &PhantomData::<I::Item>,
        );

        self.epoch += 1;

        let archetype = &mut self.archetypes[archetype_idx as usize];
        let entities = &mut self.entities;
        let epoch = self.epoch;

        SpawnBatch {
            bundles: bundles.into_iter(),
            epoch,
            archetype_idx,
            archetype,
            entities,
        }
    }

    /// Returns an iterator which spawns and yield entities
    /// using bundles returnd from provided iterator.
    ///
    /// When bundles iterator returns `None`, returned iterator returns `None` too.
    ///
    /// If bundles iterator is fused, returned iterator is fused too.
    /// If bundles iterator is double-ended, returned iterator is double-ended too.
    /// If bundles iterator has exact size, returned iterator has exact size too.
    ///
    /// Skipping items on returned iterator will not cause them to be spawned
    /// and same number of items will be skipped on bundles iterator.
    ///
    /// Returned iterator attempts to optimize storage allocation for entities
    /// if consumed with functions like `fold`, `rfold`, `for_each` or `collect`.
    ///
    /// When returned iterator is dropped, no more entities will be spawned
    /// even if bundles iterator has items left.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn spawn_batch_owning<B, I>(&mut self, bundles: I) -> SpawnBatchOwned<'_, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let archetype_idx = self.edges.spawn(
            &mut self.registry,
            &mut self.archetypes,
            &PhantomData::<I::Item>,
        );

        self.epoch += 1;

        let archetype = &mut self.archetypes[archetype_idx as usize];
        let entities = &mut self.entities;
        let epoch = self.epoch;

        SpawnBatchOwned {
            bundles: bundles.into_iter(),
            epoch,
            archetype_idx,
            archetype,
            entities,
        }
    }

    /// Despawns an entity with specified id, currently owned by the `World`.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn despawn(&mut self, entity: &EntityId) -> Result<(), OwnershipError> {
        with_encoder!(self, encoder => self.despawn_with_encoder(entity, &mut encoder))
    }

    #[cfg(feature = "rc")]
    pub(crate) fn despawn_with_encoder(
        &mut self,
        entity: &EntityId,
        encoder: &mut ActionEncoder,
    ) -> Result<(), OwnershipError> {
        let (archetype, idx) = self.entities.despawn(entity)?;

        let opt_id =
            unsafe { self.archetypes[archetype as usize].despawn_unchecked(*entity, idx, encoder) };
        if let Some(id) = opt_id {
            self.entities.set_location(id, archetype, idx)
        }

        Ok(())
    }

    /// Despawns an entity with specified id.
    #[cfg(not(feature = "rc"))]
    #[inline]
    pub fn despawn(&mut self, entity: &EntityId) -> Result<(), NoSuchEntity> {
        with_encoder!(self, encoder => self.despawn_with_encoder(entity, &mut encoder))
    }

    #[cfg(not(feature = "rc"))]
    pub fn despawn_with_encoder(
        &mut self,
        entity: &EntityId,
        encoder: &mut ActionEncoder,
    ) -> Result<(), NoSuchEntity> {
        let (archetype, idx) = self.entities.despawn(entity)?;

        let opt_id = unsafe { self.archetypes[archetype as usize].despawn_unchecked(idx, encoder) };
        if let Some(id) = opt_id {
            self.entities.set_location(id, archetype, idx)
        }

        Ok(())
    }

    /// Inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn insert<T, P>(&mut self, entity: &Entity<P>, component: T)
    where
        T: Component,
    {
        assert!(self.entities.is_owner_of(entity));
        self.try_insert(entity, component).expect("Entity exists");
    }

    /// Attemots to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn try_insert<T>(&mut self, entity: &EntityId, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        with_encoder!(self, encoder => self.try_insert_with_encoder(entity, component, &mut encoder))
    }

    #[inline]
    pub(crate) fn try_insert_with_encoder<T>(
        &mut self,
        entity: &EntityId,
        component: T,
        encoder: &mut ActionEncoder,
    ) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        let (src_archetype, idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;

        self.epoch += 1;

        if self.archetypes[src_archetype as usize].contains_id(TypeId::of::<T>()) {
            unsafe {
                self.archetypes[src_archetype as usize]
                    .set(*entity, idx, component, self.epoch, encoder);
            }

            return Ok(());
        }

        let dst_archetype =
            self.edges
                .insert::<T>(&mut self.registry, &mut self.archetypes, src_archetype);

        debug_assert_ne!(src_archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.insert(*entity, dst, idx, component, self.epoch) };

        self.entities
            .set_location(entity.idx(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(())
    }

    /// Removes component from the specified entity and returns its value.
    ///
    /// If entity does not have component of this type, fails with `Err(EntityError::MissingComponent)`.
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn remove<T>(&mut self, entity: &EntityId) -> Result<T, EntityError>
    where
        T: Component,
    {
        let (src_archetype, idx) = self.entities.get(entity).ok_or(EntityError::NoSuchEntity)?;

        self.epoch += 1;

        if !self.archetypes[src_archetype as usize].contains_id(TypeId::of::<T>()) {
            return Err(EntityError::MissingComponents);
        }

        let dst_archetype =
            self.edges
                .remove(&mut self.archetypes, src_archetype, TypeId::of::<T>());

        debug_assert_ne!(src_archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id, component) = unsafe { src.remove(*entity, dst, idx) };

        self.entities
            .set_location(entity.idx(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(component)
    }

    /// Drops component from the specified entity.
    ///
    /// If entity does not have component of this type, fails with `Err(EntityError::MissingComponent)`.
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn drop<T>(&mut self, entity: &EntityId) -> Result<(), EntityError>
    where
        T: Component,
    {
        self.drop_erased(entity, TypeId::of::<T>())
    }

    /// Drops component from the specified entity.
    ///
    /// If entity does not have component of this type, fails with `Err(EntityError::MissingComponent)`.
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn drop_erased(&mut self, entity: &EntityId, id: TypeId) -> Result<(), EntityError> {
        with_encoder!(self, encoder => self.drop_erased_with_encoder(entity, id, &mut encoder))
    }

    pub(crate) fn drop_erased_with_encoder(
        &mut self,
        entity: &EntityId,
        id: TypeId,
        encoder: &mut ActionEncoder,
    ) -> Result<(), EntityError> {
        let (src_archetype, idx) = self.entities.get(entity).ok_or(EntityError::NoSuchEntity)?;

        self.epoch += 1;

        if !self.archetypes[src_archetype as usize].contains_id(id) {
            return Err(EntityError::MissingComponents);
        }

        let dst_archetype = self.edges.remove(&mut self.archetypes, src_archetype, id);

        debug_assert_ne!(src_archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.drop_bundle(*entity, dst, idx, encoder) };

        self.entities
            .set_location(entity.idx(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(())
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn insert_bundle<B, T>(&mut self, entity: &Entity<T>, bundle: B)
    where
        B: DynamicBundle,
    {
        assert!(self.entities.is_owner_of(entity));
        self.try_insert_bundle(entity, bundle).unwrap();
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
    #[inline]
    pub fn try_insert_bundle<B>(&mut self, entity: &EntityId, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        with_encoder!(self, encoder => self.try_insert_bundle_with_encoder(entity, bundle, &mut encoder))
    }

    #[inline]
    pub(crate) fn try_insert_bundle_with_encoder<B>(
        &mut self,
        entity: &EntityId,
        bundle: B,
        encoder: &mut ActionEncoder,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        if !bundle.valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let (src_archetype, idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;

        if bundle.with_ids(|ids| ids.is_empty()) {
            return Ok(());
        }

        self.epoch += 1;

        let dst_archetype = self.edges.insert_bundle(
            &mut self.registry,
            &mut self.archetypes,
            src_archetype,
            &bundle,
        );

        if dst_archetype == src_archetype {
            unsafe {
                self.archetypes[src_archetype as usize]
                    .set_bundle(*entity, idx, bundle, self.epoch, encoder)
            };
            return Ok(());
        }

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) =
            unsafe { src.insert_bundle(*entity, dst, idx, bundle, self.epoch, encoder) };

        self.entities
            .set_location(entity.idx(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(())
    }

    /// Drops components of the specified entity with type from the bundle.
    /// Skips any component type entity doesn't have.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn drop_bundle<B>(&mut self, entity: &EntityId) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        with_encoder!(self, encoder => self.drop_bundle_with_encoder::<B>(entity, &mut encoder))
    }

    #[inline]
    pub(crate) fn drop_bundle_with_encoder<B>(
        &mut self,
        entity: &EntityId,
        encoder: &mut ActionEncoder,
    ) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        let (src_archetype, idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;

        if B::static_with_ids(|ids| {
            ids.iter()
                .all(|&id| !self.archetypes[src_archetype as usize].contains_id(id))
        }) {
            // No components to remove.
            return Ok(());
        }

        self.epoch += 1;

        let dst_archetype = self
            .edges
            .remove_bundle::<B>(&mut self.archetypes, src_archetype);

        debug_assert_ne!(src_archetype, dst_archetype);

        let (before, after) = self
            .archetypes
            .split_at_mut(src_archetype.max(dst_archetype) as usize);

        let (src, dst) = match src_archetype < dst_archetype {
            true => (&mut before[src_archetype as usize], &mut after[0]),
            false => (&mut after[0], &mut before[dst_archetype as usize]),
        };

        let (dst_idx, opt_src_id) = unsafe { src.drop_bundle(*entity, dst, idx, encoder) };

        self.entities
            .set_location(entity.idx(), dst_archetype, dst_idx);

        if let Some(src_id) = opt_src_id {
            self.entities.set_location(src_id, src_archetype, idx);
        }

        Ok(())
    }

    /// Adds relation between two entities to the [`World`]
    #[cfg(feature = "relation")]
    #[inline]
    pub fn add_relation<R, T, U>(&mut self, entity: &Entity<T>, relation: R, target: &Entity<U>)
    where
        R: Relation,
    {
        with_encoder!(self, encoder => self.add_relation_with_encoder(entity, relation, target, &mut encoder));
    }

    /// Adds relation between two entities to the [`World`]
    #[cfg(feature = "relation")]
    #[inline]
    pub fn add_relation_with_encoder<R, T, U>(
        &mut self,
        entity: &Entity<T>,
        relation: R,
        target: &Entity<U>,
        encoder: &mut ActionEncoder,
    ) where
        R: Relation,
    {
        debug_assert!(self.entities.is_owner_of(entity));
        debug_assert!(self.entities.is_owner_of(target));

        self.try_add_relation_with_encoder(entity, relation, target, encoder)
            .expect("Entities exist");
    }

    /// Adds relation between two entities to the [`World`]
    #[cfg(feature = "relation")]
    #[inline]
    pub fn try_add_relation<R>(
        &mut self,
        entity: &EntityId,
        relation: R,
        target: &EntityId,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        with_encoder!(self, encoder => self.try_add_relation_with_encoder(entity, relation, target, &mut encoder))
    }

    /// Adds relation between two entities to the [`World`]
    #[cfg(feature = "relation")]
    pub fn try_add_relation_with_encoder<R>(
        &mut self,
        entity: &EntityId,
        relation: R,
        target: &EntityId,
        encoder: &mut ActionEncoder,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        use crate::relation::{OriginComponent, TargetComponent};

        self.entities.get(entity).ok_or(NoSuchEntity)?;
        self.entities.get(target).ok_or(NoSuchEntity)?;

        self.epoch += 1;

        if R::SYMMETRIC {
            insert_relation_component(
                self,
                *entity,
                (*target, relation),
                encoder,
                |(target, relation)| OriginComponent::new(target, relation),
                |entity, component, (target, relation), encoder| {
                    component.set(entity, target, relation, encoder)
                },
            );

            insert_relation_component(
                self,
                *target,
                (*entity, relation),
                encoder,
                |(target, relation)| OriginComponent::new(target, relation),
                |entity, component, (target, relation), encoder| {
                    component.set(entity, target, relation, encoder)
                },
            );
        } else {
            insert_relation_component(
                self,
                *entity,
                (*target, relation),
                encoder,
                |(target, relation)| OriginComponent::new(target, relation),
                |entity, component, (target, relation), encoder| {
                    component.set(entity, target, relation, encoder)
                },
            );

            insert_relation_component(
                self,
                *target,
                *entity,
                encoder,
                |entity| TargetComponent::<R>::new(entity),
                |_, component, entity, _| component.set(entity),
            );
        }
        Ok(())
    }

    /// Checks that entity has components of all types from the bundle.
    /// Pins those types to the entity.
    ///
    /// Pinning serves as API level contract.
    ///
    /// Pinned components are not enforced to stay at entity
    /// and can be removed using `World::remove` or `World::remove_bundle`
    /// with a clone of the `Entity` without component types pinned.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn pin_bundle<B>(&mut self, entity: Entity) -> Entity<B>
    where
        B: Bundle,
    {
        if !B::static_valid() {
            panic!(
                "Specified bundle `{}` is not valid. Check for duplicate component types",
                type_name::<B>()
            );
        }

        assert!(self.entities.is_owner_of(&entity));

        let (archetype, _idx) = self.entities.get(&entity).unwrap();

        let archetype = &self.archetypes[archetype as usize];

        if B::static_with_ids(|ids| ids.iter().any(|&id| !archetype.contains_id(id))) {
            panic!("Attampt to pin missing components");
        }

        entity.with_bundle()
    }

    /// Queries components from specified entity.
    ///
    /// Requires access to all components in query.
    /// Components proved by entity reference must be queried or skipped in order
    /// followed by list of optionals.
    ///
    /// # Panics
    ///
    /// If `Entity` was not created by this world, this function will panic.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn get<'a, Q, A: 'a>(&'a self, entity: &Entity<A>) -> PhantomQueryItem<'a, Q>
    where
        &'a A: Proof<Q>,
        Q: ImmutablePhantomQuery,
    {
        self.query_one::<Q>(entity).expect("Query is proven")
    }

    /// Queries components from specified entity.
    ///
    /// Requires access to all components in query.
    /// Components proved by entity reference must be queried or skipped in order
    /// followed by list of optionals.
    ///
    /// # Panics
    ///
    /// If `Entity` was not created by this world, this function will panic.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn get_mut<'a, Q, A: 'a>(&'a mut self, entity: &Entity<A>) -> PhantomQueryItem<'a, Q>
    where
        &'a mut A: Proof<Q>,
        Q: PhantomQuery,
    {
        self.query_one_mut::<Q>(entity).expect("Query is proven")
    }

    /// Queries components from specified entity.
    ///
    /// If query cannot be satisfied, returns `QueryOneError::NotSatisfied`.
    #[inline]
    pub fn query_one<'a, Q>(
        &'a self,
        entity: &EntityId,
    ) -> Result<PhantomQueryItem<'a, Q>, QueryOneError>
    where
        Q: ImmutablePhantomQuery,
    {
        debug_assert!(Q::is_valid(), "Immutable queries are always valid");
        debug_assert_immutable_query(&PhantomData::<Q>);

        let (archetype, idx) = self
            .entities
            .get(entity)
            .ok_or(QueryOneError::NoSuchEntity)?;

        let archetype = &self.archetypes[archetype as usize];

        debug_assert!(archetype.len() >= idx as usize, "Entity index is valid");

        if Q::skip_archetype(archetype) {
            return Err(QueryOneError::NotSatisfied);
        }

        let mut fetch = unsafe { Q::fetch(archetype, self.epoch) };

        if unsafe { fetch.skip_chunk(chunk_idx(idx as usize)) } {
            return Err(QueryOneError::NotSatisfied);
        }

        unsafe { fetch.visit_chunk(chunk_idx(idx as usize)) }

        if unsafe { fetch.skip_item(idx as usize) } {
            return Err(QueryOneError::NotSatisfied);
        }

        let item = unsafe { fetch.get_item(idx as usize) };
        Ok(item)
    }

    /// Queries components from specified entity.
    ///
    /// If query cannot be satisfied, returns `EntityError::MissingComponents`.
    #[inline]
    pub fn query_one_mut<'a, Q>(
        &'a mut self,
        entity: &EntityId,
    ) -> Result<PhantomQueryItem<'a, Q>, QueryOneError>
    where
        Q: PhantomQuery,
    {
        assert!(Q::is_valid(), "Invalid query specified");

        self.epoch += 1;

        let (archetype, idx) = self
            .entities
            .get(entity)
            .ok_or(QueryOneError::NoSuchEntity)?;

        let archetype = &self.archetypes[archetype as usize];

        debug_assert!(archetype.len() >= idx as usize, "Entity index is valid");

        if Q::skip_archetype(archetype) {
            return Err(QueryOneError::NotSatisfied);
        }

        let mut fetch = unsafe { Q::fetch(archetype, self.epoch) };

        if unsafe { fetch.skip_chunk(chunk_idx(idx as usize)) } {
            return Err(QueryOneError::NotSatisfied);
        }

        unsafe { fetch.visit_chunk(chunk_idx(idx as usize)) }

        if unsafe { fetch.skip_item(idx as usize) } {
            return Err(QueryOneError::NotSatisfied);
        }

        let item = unsafe { fetch.get_item(idx as usize) };
        Ok(item)
    }

    /// Returns new [`Tracks`] instance to use with tracking queries.
    ///
    /// Returned [`Tracks`] instance considers only modifications
    /// that happen after this function call as "new" for the first tracking query.
    #[inline]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Run world maintenance, completing all deferred operations on it.
    ///
    /// Currently deferred operations are:
    /// * Despawn of entities with no strong references left
    #[inline]
    pub fn maintain(&mut self) {
        #[cfg(feature = "rc")]
        {
            let mut encoder = self
                .cached_encoder
                .take()
                .unwrap_or_else(ActionEncoder::new);

            let queue = self.entities.drop_queue();

            loop {
                queue.drain(&mut self.drop_queue);

                if self.drop_queue.is_empty() {
                    if encoder.execute(self) {
                        // Check again.
                        continue;
                    }

                    break;
                }

                #[inline(never)]
                #[cold]
                fn missing_entity() -> EntityId {
                    panic!("Drop queue yielded non-existing entity")
                }

                for id in self.drop_queue.drain(..) {
                    let entity = self.entities.get_entity(id).unwrap_or_else(missing_entity);

                    let (archetype, idx) = self.entities.dropped(id);
                    let opt_id = unsafe {
                        self.archetypes[archetype as usize].despawn_unchecked(
                            entity,
                            idx,
                            &mut encoder,
                        )
                    };
                    if let Some(id) = opt_id {
                        self.entities.set_location(id, archetype, idx)
                    }
                }
            }
        }
    }

    /// Transfers ownership of the entity from the caller to the `World`.
    /// After this call, entity won't be despawned until [`World::despawn`] is called with this entity id.
    #[cfg(feature = "rc")]
    pub fn keep<T>(&mut self, entity: Entity<T>) {
        assert!(self.entities.is_owner_of(&entity));
        self.entities.give_ownership(entity);
    }

    /// Transfers ownership of the entity from the `World` to the caller.
    /// After this call, entity should be despawned by dropping returned entity reference,
    /// or by returning ownership to the `World` and then called [`World::despawn`]
    ///
    /// Returns error if entity with specified id does not exists,
    /// or if that entity is not owned by the `World`.
    #[cfg(feature = "rc")]
    pub fn take(&mut self, entity: &EntityId) -> Result<Entity, OwnershipError> {
        self.entities.take_ownership(entity)
    }

    /// Checks if specified entity has componet of specified type.
    #[cfg(feature = "rc")]
    #[inline]
    pub fn has_component_owning<T: 'static, U>(&self, entity: &Entity<U>) -> bool {
        assert!(self.entities.is_owner_of(entity));

        let (archetype, _idx) = self.entities.get(entity).unwrap();
        self.archetypes[archetype as usize].contains_id(TypeId::of::<T>())
    }

    /// Attemtps to check if specified entity has componet of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline]
    pub fn has_component<T: 'static>(&self, entity: &EntityId) -> Result<bool, NoSuchEntity> {
        let (archetype, _idx) = self.entities.get(entity).ok_or(NoSuchEntity)?;
        Ok(self.archetypes[archetype as usize].contains_id(TypeId::of::<T>()))
    }

    /// Checks if specified entity is still alive.
    #[inline]
    pub fn is_alive(&self, entity: &EntityId) -> bool {
        self.entities.get(entity).is_some()
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method only works with immutable queries.
    #[inline]
    pub fn query<'a, Q>(&'a self) -> QueryRef<'a, (PhantomData<Q>,), ()>
    where
        Q: ImmutablePhantomQuery,
    {
        self.make_query(PhantomData)
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method only works with immutable queries.
    #[inline]
    pub fn make_query<'a, Q>(&'a self, query: Q) -> QueryRef<'a, (Q,), ()>
    where
        Q: ImmutableQuery,
    {
        assert!(query.is_valid(), "Invalid query specified");

        QueryRef::new(&self.archetypes, self.epoch, (query,), ())
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method can be used for queries that mutate components.
    #[inline]
    pub fn query_mut<'a, Q>(&'a mut self) -> QueryMut<'a, (PhantomData<Q>,), ()>
    where
        Q: PhantomQuery,
    {
        self.make_query_mut(PhantomData)
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method can be used for queries that mutate components.
    #[inline]
    pub fn make_query_mut<'a, Q>(&'a mut self, query: Q) -> QueryMut<'a, (Q,), ()>
    where
        Q: Query,
    {
        assert!(query.is_valid(), "Invalid query specified");

        QueryMut::new(&self.archetypes, &mut self.epoch, (query,), ())
    }

    /// Starts building immutable query.
    #[inline]
    pub fn build_query<'a>(&'a self) -> QueryRef<'a, (), ()> {
        QueryRef::new(&self.archetypes, self.epoch, (), ())
    }

    /// Returns query that iterates over all entities with specified component
    #[inline]
    pub fn filter_with<'a, T>(&'a self) -> QueryRef<'a, (), With<T>>
    where
        T: Component,
    {
        QueryRef::new(&self.archetypes, self.epoch, (), With::new())
    }

    /// Returns query that iterates over all entities without specified component
    #[inline]
    pub fn filter_without<'a, T>(&'a self) -> QueryRef<'a, (), Without<T>>
    where
        T: Component,
    {
        QueryRef::new(&self.archetypes, self.epoch, (), Without::new())
    }

    /// Returns query that iterates over all entities with specified relation,
    /// not bound to any specific entity.
    ///
    /// Yields all relation of the type on each entity.
    #[inline]
    pub fn query_relation<'a, R>(&'a self) -> QueryRef<'a, (PhantomData<QueryRelation<R>>,), ()>
    where
        QueryRelation<R>: ImmutablePhantomQuery,
    {
        QueryRef::new(&self.archetypes, self.epoch, (PhantomData,), ())
    }

    /// Returns query that iterates over all entities with specified relation,
    /// bound to one specific entity.
    ///
    /// Yields one relation of the type on each entity.
    #[inline]
    pub fn query_relation_to<'a, R>(
        &'a self,
        target: EntityId,
    ) -> QueryRef<'a, (QueryRelationTo<R>,), ()>
    where
        QueryRelationTo<R>: ImmutableQuery,
    {
        QueryRef::new(
            &self.archetypes,
            self.epoch,
            (QueryRelationTo::new(target),),
            (),
        )
    }

    /// Returns query that iterates over all entities with specified relation,
    /// bound to one specific entity.
    ///
    /// Yields nothing.
    #[inline]
    pub fn filter_relation_to<'a, R>(
        &'a self,
        target: EntityId,
    ) -> QueryRef<'a, (), FilterRelationTo<R>>
    where
        R: Relation,
    {
        QueryRef::new(&self.archetypes, self.epoch, (), filter_relation_to(target))
    }

    /// Queries the world to iterate over entities and components specified by the query type.
    ///
    /// This method only works with immutable queries.
    #[inline]
    pub fn build_query_mut<'a>(&'a mut self) -> QueryMut<'a, (), ()> {
        QueryMut::new(&self.archetypes, &mut self.epoch, (), ())
    }

    /// Returns query that iterates over all entities with specified component
    #[inline]
    pub fn query_with_mut<'a, T>(&'a mut self) -> QueryMut<'a, (), With<T>>
    where
        T: Component,
    {
        QueryMut::new(&self.archetypes, &mut self.epoch, (), With::new())
    }

    /// Returns query that iterates over all entities without specified component
    #[inline]
    pub fn query_without_mut<'a, T>(&'a mut self) -> QueryMut<'a, (), Without<T>>
    where
        T: Component,
    {
        QueryMut::new(&self.archetypes, &mut self.epoch, (), Without::new())
    }

    /// Returns query that iterates over all entities with specified relation,
    #[inline]
    pub fn query_relation_mut<'a, R>(
        &'a mut self,
    ) -> QueryMut<'a, (PhantomData<QueryRelation<R>>,), ()>
    where
        QueryRelation<R>: PhantomQuery,
    {
        QueryMut::new(&self.archetypes, &mut self.epoch, (PhantomData,), ())
    }

    /// Returns query that iterates over all entities with specified relation,
    /// bound to one specific entity.
    ///
    /// Yields one relation of the type on each entity.
    #[inline]
    pub fn query_relation_to_mut<'a, R>(
        &'a mut self,
        target: EntityId,
    ) -> QueryMut<'a, (QueryRelationTo<R>,), ()>
    where
        QueryRelationTo<R>: Query,
    {
        QueryMut::new(
            &self.archetypes,
            &mut self.epoch,
            (QueryRelationTo::new(target),),
            (),
        )
    }

    /// Returns query that iterates over all entities with specified relation,
    /// bound to one specific entity.
    ///
    /// Yields nothing.
    #[inline]
    pub fn filter_relation_to_mut<'a, R>(
        &'a mut self,
        target: EntityId,
    ) -> QueryMut<'a, (), FilterRelationTo<R>>
    where
        R: Relation,
    {
        QueryMut::new(
            &self.archetypes,
            &mut self.epoch,
            (),
            filter_relation_to(target),
        )
    }

    /// Splits the world into entity-meta and mutable query.
    /// Queries the world to iterate over entities and components specified by the query type.
    /// `EntityMeta` can be used to fetch and control some meta-information about entities while query is alive,
    /// including checking if entity is alive, checking components attached to entity and taking, giving entity ownership.
    ///
    /// This method can be used for queries that mutate components.
    #[inline]
    pub fn meta_query_mut<'a, Q>(&'a mut self, query: Q) -> (EntityMeta<'a>, QueryMut<'a, (Q,), ()>)
    where
        Q: Query,
    {
        assert!(query.is_valid(), "Invalid query specified");

        let meta = EntityMeta {
            entities: &mut self.entities,
            archetypes: &self.archetypes,
        };
        let query = QueryMut::new(&self.archetypes, &mut self.epoch, (query,), ());
        (meta, query)
    }
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
#[allow(missing_debug_implementations)]
pub struct SpawnBatch<'a, I> {
    bundles: I,
    epoch: u64,
    archetype_idx: u32,
    archetype: &'a mut Archetype,
    entities: &'a mut Entities,
}

impl<B, I> SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    /// Spawns the rest of the entities, dropping their ids.
    ///
    /// Note that `SpawnBatchOwned` does not have this methods
    /// as dropped `Entity` references would cause spawned entities
    /// to be despawned, and that's probably not what user wants.
    pub fn spawn_all(mut self) {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.for_each(|bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.idx(), archetype_idx, idx);
        })
    }
}

impl<B, I> Iterator for SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = EntityId;

    fn next(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next()?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx(), self.archetype_idx, idx);

        Some(entity)
    }

    fn nth(&mut self, n: usize) -> Option<EntityId> {
        // No reason to create entities
        // for which the only reference is immediatelly dropped
        let bundle = self.bundles.nth(n)?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx(), self.archetype_idx, idx);

        Some(entity)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, EntityId) -> T,
    {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.fold(init, |acc, bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.idx(), archetype_idx, idx);
            f(acc, entity)
        })
    }

    fn collect<T>(self) -> T
    where
        T: FromIterator<EntityId>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.
        spawn_reserve(&self.bundles, self.archetype);

        FromIterator::from_iter(self)
    }
}

impl<B, I> ExactSizeIterator for SpawnBatch<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle,
{
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<B, I> DoubleEndedIterator for SpawnBatch<'_, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next_back()?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx(), self.archetype_idx, idx);

        Some(entity)
    }

    fn nth_back(&mut self, n: usize) -> Option<EntityId> {
        // No reason to create entities
        // for which the only reference is immediatelly dropped
        let bundle = self.bundles.nth_back(n)?;

        let entity = self.entities.spawn();
        let idx = self.archetype.spawn(entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx(), self.archetype_idx, idx);

        Some(entity)
    }

    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, EntityId) -> T,
    {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.rfold(init, |acc, bundle| {
            let entity = entities.spawn();
            let idx = archetype.spawn(entity, bundle, epoch);
            entities.set_location(entity.idx(), archetype_idx, idx);
            f(acc, entity)
        })
    }
}

impl<B, I> FusedIterator for SpawnBatch<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle,
{
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
#[cfg(feature = "rc")]
#[allow(missing_debug_implementations)]
pub struct SpawnBatchOwned<'a, I> {
    bundles: I,
    epoch: u64,
    archetype_idx: u32,
    archetype: &'a mut Archetype,
    entities: &'a mut Entities,
}

#[cfg(feature = "rc")]
impl<B, I> Iterator for SpawnBatchOwned<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        let bundle = self.bundles.next()?;

        let entity = self.entities.spawn_owning();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx(), self.archetype_idx, idx);

        Some(entity)
    }

    fn nth(&mut self, n: usize) -> Option<Entity> {
        // No reason to create entities
        // for which the only reference is immediatelly dropped
        let bundle = self.bundles.nth(n)?;

        let entity = self.entities.spawn_owning();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx(), self.archetype_idx, idx);

        Some(entity)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, Entity) -> T,
    {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.fold(init, |acc, bundle| {
            let entity = entities.spawn_owning();
            let idx = archetype.spawn(*entity, bundle, epoch);
            entities.set_location(entity.idx(), archetype_idx, idx);
            f(acc, entity)
        })
    }

    fn collect<T>(self) -> T
    where
        T: FromIterator<Entity>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.
        spawn_reserve(&self.bundles, self.archetype);

        FromIterator::from_iter(self)
    }

    fn count(self) -> usize {
        // Entities are conceptually despawned immediately.
        // Just report the count in this case.
        self.bundles.count()
    }
}

#[cfg(feature = "rc")]
impl<B, I> ExactSizeIterator for SpawnBatchOwned<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle,
{
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

#[cfg(feature = "rc")]
impl<B, I> DoubleEndedIterator for SpawnBatchOwned<'_, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<Entity> {
        let bundle = self.bundles.next_back()?;

        let entity = self.entities.spawn_owning();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx(), self.archetype_idx, idx);

        Some(entity)
    }

    fn nth_back(&mut self, n: usize) -> Option<Entity> {
        // No reason to create entities
        // for which the only reference is immediatelly dropped
        let bundle = self.bundles.nth_back(n)?;

        let entity = self.entities.spawn_owning();
        let idx = self.archetype.spawn(*entity, bundle, self.epoch);

        self.entities
            .set_location(entity.idx(), self.archetype_idx, idx);

        Some(entity)
    }

    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, Entity) -> T,
    {
        spawn_reserve(&self.bundles, self.archetype);

        let entities = &mut self.entities;
        let archetype = &mut self.archetype;
        let archetype_idx = self.archetype_idx;
        let epoch = self.epoch;

        self.bundles.rfold(init, |acc, bundle| {
            let entity = entities.spawn_owning();
            let idx = archetype.spawn(*entity, bundle, epoch);
            entities.set_location(entity.idx(), archetype_idx, idx);
            f(acc, entity)
        })
    }
}

#[cfg(feature = "rc")]
impl<B, I> FusedIterator for SpawnBatchOwned<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle,
{
}

/// Error returned in case specified [`EntityId`]
/// does not reference any live entity in the [`World`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoSuchEntity;

impl fmt::Display for NoSuchEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Speicified entity is not found")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NoSuchEntity {}

/// Error returned in case specified entity does not contain
/// component of required type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MissingComponents;

impl fmt::Display for MissingComponents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Speicified component is not found in entity")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MissingComponents {}

/// Error returned if either entity reference is invalid
/// or component of required type is not found for an entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityError {
    /// Error returned in case specified [`EntityId`]
    /// does not reference any live entity in the [`World`].
    NoSuchEntity,

    /// Error returned in case specified entity does not contain
    /// component of required type.
    MissingComponents,
}

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchEntity => fmt::Display::fmt(&NoSuchEntity, f),
            Self::MissingComponents => fmt::Display::fmt(&MissingComponents, f),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EntityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoSuchEntity => Some(&NoSuchEntity),
            Self::MissingComponents => Some(&MissingComponents),
        }
    }
}

impl From<NoSuchEntity> for EntityError {
    fn from(_: NoSuchEntity) -> Self {
        EntityError::NoSuchEntity
    }
}

impl From<MissingComponents> for EntityError {
    fn from(_: MissingComponents) -> Self {
        EntityError::MissingComponents
    }
}

impl PartialEq<NoSuchEntity> for EntityError {
    fn eq(&self, _: &NoSuchEntity) -> bool {
        matches!(self, EntityError::NoSuchEntity)
    }
}

impl PartialEq<MissingComponents> for EntityError {
    fn eq(&self, _: &MissingComponents) -> bool {
        matches!(self, EntityError::MissingComponents)
    }
}

/// Error returned by [`query_one_*`] method family
/// when query is not satisfied by the entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QueryOneError {
    /// Error returned in case specified [`EntityId`]
    /// does not reference any live entity in the [`World`].
    NoSuchEntity,

    /// Error returned in case specified entity does not contain
    /// component of required type.
    NotSatisfied,
}

impl fmt::Display for QueryOneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchEntity => fmt::Display::fmt(&NoSuchEntity, f),
            Self::NotSatisfied => f.write_str("Query is not satisfied"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for QueryOneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoSuchEntity => Some(&NoSuchEntity),
            Self::NotSatisfied => None,
        }
    }
}

impl From<NoSuchEntity> for QueryOneError {
    fn from(_: NoSuchEntity) -> Self {
        QueryOneError::NoSuchEntity
    }
}

impl PartialEq<NoSuchEntity> for QueryOneError {
    fn eq(&self, _: &NoSuchEntity) -> bool {
        matches!(self, QueryOneError::NoSuchEntity)
    }
}

/// Error that may occur when function expects `World` to own an entity with specific id.
#[cfg(feature = "rc")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OwnershipError {
    /// Error returned in case specified [`EntityId`]
    /// does not reference any live entity in the [`World`].
    NoSuchEntity,

    /// Error returned in case specified [`EntityId`]
    /// does not reference an entity currently owned by [`World`].
    NotOwned,
}

#[cfg(feature = "rc")]
impl From<NoSuchEntity> for OwnershipError {
    fn from(_: NoSuchEntity) -> Self {
        OwnershipError::NoSuchEntity
    }
}

#[cfg(feature = "rc")]
impl PartialEq<NoSuchEntity> for OwnershipError {
    fn eq(&self, _: &NoSuchEntity) -> bool {
        matches!(self, OwnershipError::NoSuchEntity)
    }
}

#[cfg(feature = "rc")]
impl fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchEntity => fmt::Display::fmt(&NoSuchEntity, f),
            Self::NotOwned => f.write_str("Entity is not owned by World"),
        }
    }
}

#[cfg(feature = "rc")]
#[cfg(feature = "std")]
impl std::error::Error for OwnershipError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoSuchEntity => Some(&NoSuchEntity),
            _ => None,
        }
    }
}

/// Inserts relation component.
/// This function uses different code to assign component when it already exists on entity.
fn insert_relation_component<T, C>(
    world: &mut World,
    entity: EntityId,
    value: T,
    encoder: &mut ActionEncoder,
    into_component: impl FnOnce(T) -> C,
    set_component: impl FnOnce(EntityId, &mut C, T, &mut ActionEncoder),
) where
    C: Component,
{
    let (src_archetype, idx) = world.entities.get(&entity).unwrap();

    if world.archetypes[src_archetype as usize].contains_id(TypeId::of::<C>()) {
        let component =
            unsafe { world.archetypes[src_archetype as usize].get_mut::<C>(idx, world.epoch) };

        set_component(entity, component, value, encoder);

        return;
    }

    let component = into_component(value);

    let dst_archetype =
        world
            .edges
            .insert::<C>(&mut world.registry, &mut world.archetypes, src_archetype);

    debug_assert_ne!(src_archetype, dst_archetype);

    let (before, after) = world
        .archetypes
        .split_at_mut(src_archetype.max(dst_archetype) as usize);

    let (src, dst) = match src_archetype < dst_archetype {
        true => (&mut before[src_archetype as usize], &mut after[0]),
        false => (&mut after[0], &mut before[dst_archetype as usize]),
    };

    let (dst_idx, opt_src_id) = unsafe { src.insert(entity, dst, idx, component, world.epoch) };

    world
        .entities
        .set_location(entity.idx(), dst_archetype, dst_idx);

    if let Some(src_id) = opt_src_id {
        world.entities.set_location(src_id, src_archetype, idx);
    }
}
