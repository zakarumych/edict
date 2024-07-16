use core::{any::TypeId, iter::FusedIterator};

use alloc::collections::VecDeque;
use smallvec::SmallVec;

use crate::{
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle},
    component::{Component, ComponentInfo, ComponentRegistry},
    entity::{Entity, EntityId, EntityLoc, EntitySet},
    relation::Relation,
    type_id,
    world::{iter_reserve_hint, World},
};

use super::{buffer::LocalActionBuffer, ActionBuffer, ActionFn, LocalActionFn};

/// Encoder for actions that require mutable access to [`World`],
/// like spawning/despawning entities and inserting/removing/dropping components and relations.
///
/// Systems may declare `ActionEncoder` argument to record actions that will be executed later.
/// Each system will get its own `ActionEncoder` instance, so no conflicts will be caused by this argument.
/// In contract `&mut World` argument will cause system to conflict with all other systems, reducing parallelism.
///
/// Provided to component and relation hooks.
pub struct ActionEncoder<'a> {
    actions: &'a mut VecDeque<ActionFn<'static>>,
    entities: &'a EntitySet,
}

impl<'a> ActionEncoder<'a> {
    /// Returns new [`ActionEncoder`] that records actions into provided [`ActionBuffer`].
    #[inline(always)]
    pub(crate) fn new(buffer: &'a mut ActionBuffer, entities: &'a EntitySet) -> Self {
        ActionEncoder {
            actions: buffer.actions(),
            entities,
        }
    }

    /// Returns `true` if attached action buffer is empty.
    /// That is, no actions were recorded.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Allocates new entity id that can be used with following actions.
    #[inline(always)]
    pub fn allocate(&self) -> EntityLoc<'_> {
        self.entities.alloc()
    }

    /// Allocates new entity id and encodes an action to insert bundle to the entity.
    #[inline(always)]
    pub fn spawn_one<T>(&mut self, component: T) -> EntityLoc<'_>
    where
        T: Component + Send + 'static,
    {
        let entity = self.entities.alloc();
        self.insert(entity, component);
        entity
    }

    /// Allocates new entity id and encodes an action to insert bundle to the entity.
    #[inline(always)]
    pub fn spawn<B>(&mut self, bundle: B) -> EntityLoc<'_>
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        let entity = self.entities.alloc();
        self.insert_bundle(entity, bundle);
        entity
    }

    /// Allocates new entity id and encodes an action to insert bundle to the entity.
    #[inline(always)]
    pub fn spawn_external<B>(&mut self, bundle: B) -> EntityLoc<'_>
    where
        B: DynamicBundle + Send + 'static,
    {
        let entity = self.entities.alloc();
        self.insert_external_bundle(entity, bundle);
        entity
    }

    /// Returns an iterator which encodes action to spawn and yield entities
    /// using bundles yielded from provided bundles iterator.
    #[inline(always)]
    pub fn spawn_batch<I>(&mut self, bundles: I) -> SpawnBatch<I>
    where
        I: IntoIterator,
        I::Item: ComponentBundle + Send + 'static,
    {
        self.push_fn(|world| {
            world.ensure_bundle_registered::<I::Item>();
        });

        SpawnBatch {
            bundles,
            encoder: self.reborrow(),
        }
    }

    /// Returns an iterator which encodes action to spawn and yield entities
    /// using bundles yielded from provided bundles iterator.
    #[inline(always)]
    pub fn spawn_external_batch<I>(&mut self, bundles: I) -> SpawnBatch<I>
    where
        I: IntoIterator,
        I::Item: Bundle + Send + 'static,
    {
        SpawnBatch {
            bundles,
            encoder: self.reborrow(),
        }
    }

    /// Encodes an action to despawn specified entity.
    #[inline(always)]
    pub fn despawn(&mut self, entity: impl Entity) {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.despawn(id);
        })
    }

    /// Encodes an action to despawn entities in batch.
    #[inline(always)]
    pub fn despawn_batch(&mut self, entities: impl IntoIterator<Item = EntityId>) {
        let entities = entities.into_iter();

        match entities.size_hint() {
            (_, Some(upper)) if upper <= 8 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 8]>>();
                self.push_fn(move |world| {
                    let _ = world.despawn_batch(entities);
                });
            }
            (_, Some(upper)) if upper <= 16 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 16]>>();
                self.push_fn(move |world| {
                    let _ = world.despawn_batch(entities);
                });
            }
            (_, Some(upper)) if upper <= 32 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 32]>>();
                self.push_fn(move |world| {
                    let _ = world.despawn_batch(entities);
                });
            }
            _ => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 64]>>();
                self.push_fn(move |world| {
                    let _ = world.despawn_batch(entities);
                });
            }
        }
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub fn insert<T>(&mut self, entity: impl Entity, component: T)
    where
        T: Component + Send,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.insert(id, component);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub fn insert_external<T>(&mut self, entity: impl Entity, component: T)
    where
        T: Send + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.insert_external(id, component);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub fn with<F, T>(&mut self, entity: impl Entity, f: F)
    where
        F: FnOnce() -> T + Send + 'static,
        T: Component,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.with(id, f);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub fn with_external<F, T>(&mut self, entity: impl Entity, f: F)
    where
        F: FnOnce() -> T + Send + 'static,
        T: 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.with_external(id, f);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline(always)]
    pub fn insert_bundle<B>(&mut self, entity: impl Entity, bundle: B)
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.insert_bundle(id, bundle);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline(always)]
    pub fn insert_external_bundle<B>(&mut self, entity: impl Entity, bundle: B)
    where
        B: DynamicBundle + Send + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.insert_external_bundle(id, bundle);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline(always)]
    pub fn with_bundle<B>(&mut self, entity: impl Entity, bundle: B)
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.with_bundle(id, bundle);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline(always)]
    pub fn with_external_bundle<B>(&mut self, entity: impl Entity, bundle: B)
    where
        B: DynamicBundle + Send + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.with_external_bundle(id, bundle);
        });
    }

    /// Encodes an action to drop component from specified entity.
    #[inline(always)]
    pub fn drop<T>(&mut self, entity: impl Entity)
    where
        T: 'static,
    {
        self.drop_erased(entity, type_id::<T>())
    }

    /// Encodes an action to drop component from entities in batch.
    #[inline(always)]
    pub fn drop_batch<T>(&mut self, entities: impl IntoIterator<Item = EntityId>)
    where
        T: 'static,
    {
        self.drop_erased_batch(entities, type_id::<T>())
    }

    /// Encodes an action to drop component from specified entity.
    #[inline(always)]
    pub fn drop_erased(&mut self, entity: impl Entity, ty: TypeId) {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.drop_erased(id, ty);
        })
    }

    /// Encodes an action to drop component from entities in batch.
    #[inline(always)]
    pub fn drop_erased_batch(&mut self, entities: impl IntoIterator<Item = EntityId>, ty: TypeId) {
        let entities = entities.into_iter();

        match entities.size_hint() {
            (_, Some(upper)) if upper <= 8 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 8]>>();
                self.push_fn(move |world| {
                    let _ = world.drop_erased_batch(entities, ty);
                });
            }
            (_, Some(upper)) if upper <= 16 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 16]>>();
                self.push_fn(move |world| {
                    let _ = world.drop_erased_batch(entities, ty);
                });
            }
            (_, Some(upper)) if upper <= 32 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 32]>>();
                self.push_fn(move |world| {
                    let _ = world.drop_erased_batch(entities, ty);
                });
            }
            _ => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 64]>>();
                self.push_fn(move |world| {
                    let _ = world.drop_erased_batch(entities, ty);
                });
            }
        }
    }

    /// Encodes an action to drop bundle of components from specified entity.
    #[inline(always)]
    pub fn drop_bundle<B>(&mut self, entity: impl Entity)
    where
        B: Bundle,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.drop_bundle::<B>(id);
        });
    }

    /// Encodes an action to add relation between two entities to the [`World`].
    #[inline(always)]
    pub fn insert_relation<R>(&mut self, origin: impl Entity, relation: R, target: impl Entity)
    where
        R: Relation + Send,
    {
        let origin = origin.id();
        let target = target.id();
        self.push_fn(move |world| {
            let _ = world.insert_relation(origin, relation, target);
        });
    }

    /// Encodes an action to drop relation between two entities in the [`World`].
    #[inline(always)]
    pub fn drop_relation<R>(&mut self, origin: EntityId, target: EntityId)
    where
        R: Relation,
    {
        self.push_fn(move |world| {
            let _ = world.remove_relation::<R>(origin, target);
        });
    }

    /// Checks if entity is alive.
    #[inline(always)]
    pub fn is_alive(&self, entity: impl Entity) -> bool {
        entity.is_alive(self.entities)
    }

    /// Encodes action to insert resource instance.
    #[inline(always)]
    pub fn insert_resource<T>(&mut self, resource: T)
    where
        T: Send + 'static,
    {
        self.push_fn(move |world| {
            world.insert_resource(resource);
        });
    }

    /// Encodes an action to drop resource instance.
    #[inline(always)]
    pub fn drop_resource<T: 'static>(&mut self) {
        self.push_fn(move |world| {
            world.remove_resource::<T>();
        });
    }

    /// Encodes a custom action with a closure that takes reference to `World`
    /// and [`ActionEncoder`] that can be used to record new actions.
    #[inline(always)]
    pub fn closure(&mut self, fun: impl FnOnce(&mut World) + Send + 'static) {
        self.push_fn(fun);
    }

    /// Creates new [`ActionEncoder`] that records actions into the same [`ActionBuffer`]
    /// as this one.
    #[inline(always)]
    pub fn reborrow(&mut self) -> ActionEncoder {
        ActionEncoder {
            actions: self.actions,
            entities: self.entities,
        }
    }

    /// Encodes an action to remove component from specified entity.
    #[inline(always)]
    fn push_fn(&mut self, fun: impl FnOnce(&mut World) + Send + 'static) {
        self.actions.push_back(ActionFn::new(fun));
    }
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
pub struct SpawnBatch<'a, I> {
    bundles: I,
    encoder: ActionEncoder<'a>,
}

impl<B, I> SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle + Send + 'static,
{
    /// Spawns the rest of the entities, dropping their ids.
    #[inline(always)]
    pub fn spawn_all(self) {
        self.for_each(|_| {});
    }
}

impl<'a, B, I> Iterator for SpawnBatch<'a, I>
where
    I: Iterator<Item = B>,
    B: Bundle + Send + 'static,
{
    type Item = EntityId;

    #[inline(always)]
    fn next(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next()?;
        Some(self.encoder.spawn_external(bundle).id())
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<EntityId> {
        // `SpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth(n)?;
        Some(self.encoder.spawn_external(bundle).id())
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    #[inline(always)]
    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, EntityId) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles.fold(init, |acc, bundle| {
            f(acc, self.encoder.spawn_external(bundle).id())
        })
    }

    #[inline(always)]
    fn collect<T>(mut self) -> T
    where
        T: FromIterator<EntityId>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.

        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        FromIterator::from_iter(self)
    }
}

impl<B, I> ExactSizeIterator for SpawnBatch<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle + Send + 'static,
{
    #[inline(always)]
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<'a, B, I> DoubleEndedIterator for SpawnBatch<'a, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
    #[inline(always)]
    fn next_back(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next_back()?;
        Some(self.encoder.spawn_external(bundle).id())
    }

    #[inline(always)]
    fn nth_back(&mut self, n: usize) -> Option<EntityId> {
        // `SpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth_back(n)?;
        Some(self.encoder.spawn_external(bundle).id())
    }

    #[inline(always)]
    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, EntityId) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles.rfold(init, |acc, bundle| {
            f(acc, self.encoder.spawn_external(bundle).id())
        })
    }
}

impl<B, I> FusedIterator for SpawnBatch<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
}

/// Encoder for actions that require mutable access to [`World`],
/// like spawning/despawning entities and inserting/removing/dropping components and relations.
///
/// Systems may declare `ActionEncoder` argument to record actions that will be executed later.
/// Each system will get its own `ActionEncoder` instance, so no conflicts will be caused by this argument.
/// In contract `&mut World` argument will cause system to conflict with all other systems, reducing parallelism.
///
/// Provided to component and relation hooks.
pub struct LocalActionEncoder<'a> {
    actions: &'a mut VecDeque<LocalActionFn<'static>>,
    entities: &'a EntitySet,
}

impl<'a> LocalActionEncoder<'a> {
    /// Returns new [`ActionEncoder`] that records actions into provided [`ActionBuffer`].
    #[inline(always)]
    pub(crate) fn new(buffer: &'a mut LocalActionBuffer, entities: &'a EntitySet) -> Self {
        LocalActionEncoder {
            actions: buffer.actions(),
            entities,
        }
    }

    /// Returns `true` if attached action buffer is empty.
    /// That is, no actions were recorded.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Allocates new entity id that can be used with following actions.
    #[inline(always)]
    pub fn allocate(&self) -> EntityLoc<'_> {
        self.entities.alloc()
    }

    /// Allocates new entity id and encodes an action to insert bundle to the entity.
    #[inline(always)]
    pub fn spawn_one<T>(&mut self, component: T) -> EntityLoc<'_>
    where
        T: Component + 'static,
    {
        let entity = self.entities.alloc();
        self.insert(entity, component);
        entity
    }

    /// Allocates new entity id and encodes an action to insert bundle to the entity.
    #[inline(always)]
    pub fn spawn<B>(&mut self, bundle: B) -> EntityLoc<'_>
    where
        B: DynamicComponentBundle + 'static,
    {
        let entity = self.entities.alloc();
        self.insert_bundle(entity, bundle);
        entity
    }

    /// Allocates new entity id and encodes an action to insert bundle to the entity.
    #[inline(always)]
    pub fn spawn_external<B>(&mut self, bundle: B) -> EntityLoc<'_>
    where
        B: DynamicBundle + 'static,
    {
        let entity = self.entities.alloc();
        self.insert_external_bundle(entity, bundle);
        entity
    }

    /// Returns an iterator which encodes action to spawn and yield entities
    /// using bundles yielded from provided bundles iterator.
    #[inline(always)]
    pub fn spawn_batch<I>(&mut self, bundles: I) -> LocalSpawnBatch<I>
    where
        I: IntoIterator,
        I::Item: ComponentBundle + 'static,
    {
        self.push_fn(|world| {
            world.ensure_bundle_registered::<I::Item>();
        });

        LocalSpawnBatch {
            bundles,
            encoder: self.reborrow(),
        }
    }

    /// Returns an iterator which encodes action to spawn and yield entities
    /// using bundles yielded from provided bundles iterator.
    #[inline(always)]
    pub fn spawn_external_batch<I>(&mut self, bundles: I) -> LocalSpawnBatch<I>
    where
        I: IntoIterator,
        I::Item: Bundle + 'static,
    {
        LocalSpawnBatch {
            bundles,
            encoder: self.reborrow(),
        }
    }

    /// Encodes an action to despawn specified entity.
    #[inline(always)]
    pub fn despawn(&mut self, entity: impl Entity) {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.despawn(id);
        })
    }

    /// Encodes an action to despawn entities in batch.
    #[inline(always)]
    pub fn despawn_batch(&mut self, entities: impl IntoIterator<Item = EntityId>) {
        let entities = entities.into_iter();

        match entities.size_hint() {
            (_, Some(upper)) if upper <= 8 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 8]>>();
                self.push_fn(move |world| {
                    let _ = world.despawn_batch(entities);
                });
            }
            (_, Some(upper)) if upper <= 16 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 16]>>();
                self.push_fn(move |world| {
                    let _ = world.despawn_batch(entities);
                });
            }
            (_, Some(upper)) if upper <= 32 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 32]>>();
                self.push_fn(move |world| {
                    let _ = world.despawn_batch(entities);
                });
            }
            _ => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 64]>>();
                self.push_fn(move |world| {
                    let _ = world.despawn_batch(entities);
                });
            }
        }
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub fn insert<T>(&mut self, entity: impl Entity, component: T)
    where
        T: Component,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.insert(id, component);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub fn insert_external<T>(&mut self, entity: impl Entity, component: T)
    where
        T: 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.insert_external(id, component);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub fn with<T>(&mut self, entity: impl Entity, f: impl FnOnce() -> T + 'static)
    where
        T: Component,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.with(id, f);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub fn with_external<T>(&mut self, entity: impl Entity, f: impl FnOnce() -> T + 'static)
    where
        T: 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.with_external(id, f);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline(always)]
    pub(crate) fn _with<F, T>(
        &mut self,
        entity: impl Entity,
        f: impl FnOnce() -> T + 'static,
        replace: bool,
        get_or_register: F,
    ) where
        F: FnOnce(&mut ComponentRegistry) -> &ComponentInfo + 'static,
        T: 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world._with(id, f, replace, get_or_register);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline(always)]
    pub fn insert_bundle<B>(&mut self, entity: impl Entity, bundle: B)
    where
        B: DynamicComponentBundle + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.insert_bundle(id, bundle);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline(always)]
    pub fn insert_external_bundle<B>(&mut self, entity: impl Entity, bundle: B)
    where
        B: DynamicBundle + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.insert_external_bundle(id, bundle);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline(always)]
    pub fn with_bundle<B>(&mut self, entity: impl Entity, bundle: B)
    where
        B: DynamicComponentBundle + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.with_bundle(id, bundle);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline(always)]
    pub fn with_external_bundle<B>(&mut self, entity: impl Entity, bundle: B)
    where
        B: DynamicBundle + 'static,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.with_external_bundle(id, bundle);
        });
    }

    /// Encodes an action to drop component from specified entity.
    #[inline(always)]
    pub fn drop<T>(&mut self, entity: impl Entity)
    where
        T: 'static,
    {
        self.drop_erased(entity, type_id::<T>())
    }

    /// Encodes an action to drop component from entities in batch.
    #[inline(always)]
    pub fn drop_batch<T>(&mut self, entities: impl IntoIterator<Item = EntityId>)
    where
        T: 'static,
    {
        self.drop_erased_batch(entities, type_id::<T>())
    }

    /// Encodes an action to drop component from specified entity.
    #[inline(always)]
    pub fn drop_erased(&mut self, entity: impl Entity, ty: TypeId) {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.drop_erased(id, ty);
        })
    }

    /// Encodes an action to drop component from entities in batch.
    #[inline(always)]
    pub fn drop_erased_batch(&mut self, entities: impl IntoIterator<Item = EntityId>, ty: TypeId) {
        let entities = entities.into_iter();

        match entities.size_hint() {
            (_, Some(upper)) if upper <= 8 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 8]>>();
                self.push_fn(move |world| {
                    let _ = world.drop_erased_batch(entities, ty);
                });
            }
            (_, Some(upper)) if upper <= 16 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 16]>>();
                self.push_fn(move |world| {
                    let _ = world.drop_erased_batch(entities, ty);
                });
            }
            (_, Some(upper)) if upper <= 32 => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 32]>>();
                self.push_fn(move |world| {
                    let _ = world.drop_erased_batch(entities, ty);
                });
            }
            _ => {
                let entities = entities.into_iter().collect::<SmallVec<[_; 64]>>();
                self.push_fn(move |world| {
                    let _ = world.drop_erased_batch(entities, ty);
                });
            }
        }
    }

    /// Encodes an action to drop bundle of components from specified entity.
    #[inline(always)]
    pub fn drop_bundle<B>(&mut self, entity: impl Entity)
    where
        B: Bundle,
    {
        let id = entity.id();
        self.push_fn(move |world| {
            let _ = world.drop_bundle::<B>(id);
        });
    }

    /// Encodes an action to add relation between two entities to the [`World`].
    #[inline(always)]
    pub fn insert_relation<R>(&mut self, origin: impl Entity, relation: R, target: impl Entity)
    where
        R: Relation,
    {
        let origin = origin.id();
        let target = target.id();
        self.push_fn(move |world| {
            let _ = world.insert_relation(origin, relation, target);
        });
    }

    /// Encodes an action to drop relation between two entities in the [`World`].
    #[inline(always)]
    pub fn drop_relation<R>(&mut self, origin: EntityId, target: EntityId)
    where
        R: Relation,
    {
        self.push_fn(move |world| {
            let _ = world.remove_relation::<R>(origin, target);
        });
    }

    /// Checks if entity is alive.
    #[inline(always)]
    pub fn is_alive(&self, entity: impl Entity) -> bool {
        entity.is_alive(self.entities)
    }

    /// Encodes action to insert resource instance.
    #[inline(always)]
    pub fn insert_resource<T>(&mut self, resource: T)
    where
        T: 'static,
    {
        self.push_fn(move |world| {
            world.insert_resource(resource);
        });
    }

    /// Encodes an action to drop resource instance.
    #[inline(always)]
    pub fn drop_resource<T: 'static>(&mut self) {
        self.push_fn(move |world| {
            world.remove_resource::<T>();
        });
    }

    /// Encodes a custom action with a closure that takes reference to `World`
    /// and [`LocalActionEncoder`] that can be used to record new actions.
    #[inline(always)]
    pub fn closure(&mut self, fun: impl FnOnce(&mut World) + 'static) {
        self.push_fn(fun);
    }

    /// Creates new [`LocalActionEncoder`] that records actions into the same [`ActionBuffer`]
    /// as this one.
    #[inline(always)]
    pub fn reborrow(&mut self) -> LocalActionEncoder {
        LocalActionEncoder {
            actions: self.actions,
            entities: self.entities,
        }
    }

    /// Encodes an action to remove component from specified entity.
    #[inline(always)]
    fn push_fn(&mut self, fun: impl FnOnce(&mut World) + 'static) {
        self.actions.push_back(LocalActionFn::new(fun));
    }
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
pub struct LocalSpawnBatch<'a, I> {
    bundles: I,
    encoder: LocalActionEncoder<'a>,
}

impl<B, I> LocalSpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle + Send + 'static,
{
    /// Spawns the rest of the entities, dropping their ids.
    #[inline(always)]
    pub fn spawn_all(self) {
        self.for_each(|_| {});
    }
}

impl<'a, B, I> Iterator for LocalSpawnBatch<'a, I>
where
    I: Iterator<Item = B>,
    B: Bundle + Send + 'static,
{
    type Item = EntityId;

    #[inline(always)]
    fn next(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next()?;
        Some(self.encoder.spawn_external(bundle).id())
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<EntityId> {
        // `LocalSpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth(n)?;
        Some(self.encoder.spawn_external(bundle).id())
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    #[inline(always)]
    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, EntityId) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles.fold(init, |acc, bundle| {
            f(acc, self.encoder.spawn_external(bundle).id())
        })
    }

    #[inline(always)]
    fn collect<T>(mut self) -> T
    where
        T: FromIterator<EntityId>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.

        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        FromIterator::from_iter(self)
    }
}

impl<B, I> ExactSizeIterator for LocalSpawnBatch<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle + Send + 'static,
{
    #[inline(always)]
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<'a, B, I> DoubleEndedIterator for LocalSpawnBatch<'a, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
    #[inline(always)]
    fn next_back(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next_back()?;
        Some(self.encoder.spawn_external(bundle).id())
    }

    #[inline(always)]
    fn nth_back(&mut self, n: usize) -> Option<EntityId> {
        // `LocalSpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth_back(n)?;
        Some(self.encoder.spawn_external(bundle).id())
    }

    #[inline(always)]
    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, EntityId) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles.rfold(init, |acc, bundle| {
            f(acc, self.encoder.spawn_external(bundle).id())
        })
    }
}

impl<B, I> FusedIterator for LocalSpawnBatch<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
}
