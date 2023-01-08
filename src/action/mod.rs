//! This module contains definitions for action recording.
//! Actions can be recorded into [`ActionEncoder`] and executed later onto the [`World`].
//! Two primary use cases for actions are:
//! * Deferring [`World`] mutations when [`World`] is borrowed immutably, like in most [`Systems`]
//! * Generating commands in custom component drop-glue.
//!
//! [`Systems`]: edict::system::System

use core::{any::TypeId, iter::FusedIterator};

use alloc::collections::VecDeque;

use crate::{
    bundle::{Bundle, DynamicComponentBundle},
    component::Component,
    entity::{EntityId, EntitySet},
    world::{iter_reserve_hint, World},
    ComponentBundle, DynamicBundle, NoSuchEntity, Relation,
};

tiny_fn::tiny_fn! {
    struct ActionFn = FnOnce(world: &mut World, buffer: &mut ActionBuffer) | + Send;
}

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
    /// Returns new [`ActionEncoder`] that records commands into provided [`ActionBuffer`].
    #[inline]
    pub(crate) fn new(buffer: &'a mut ActionBuffer, entities: &'a EntitySet) -> Self {
        ActionEncoder {
            actions: &mut buffer.actions,
            entities,
        }
    }

    /// Returns `true` if attached action buffer is empty.
    /// That is, no actions were recorded.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Allocates new entity id that can be used with following actions.
    #[inline]
    pub fn allocate(&mut self) -> EntityId {
        self.entities.alloc()
    }

    /// Allocates new entity id and encodes an action to insert bundle to the entity.
    #[inline]
    pub fn spawn<B>(&mut self, bundle: B) -> EntityId
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        let id = self.entities.alloc();
        self.insert_bundle(id, bundle);
        id
    }

    /// Allocates new entity id and encodes an action to insert bundle to the entity.
    #[inline]
    pub fn spawn_external<B>(&mut self, bundle: B) -> EntityId
    where
        B: DynamicBundle + Send + 'static,
    {
        let id = self.entities.alloc();
        self.insert_external_bundle(id, bundle);
        id
    }

    /// Returns an iterator which encodes action to spawn and yield entities
    /// using bundles yielded from provided bundles iterator.
    #[inline]
    pub fn spawn_batch<I>(&mut self, bundles: I) -> SpawnBatch<I>
    where
        I: IntoIterator,
        I::Item: ComponentBundle + Send + 'static,
    {
        self.push_fn(|world, _| {
            world.ensure_bundle_registered::<I::Item>();
        });

        SpawnBatch {
            bundles,
            encoder: self.reborrow(),
        }
    }

    /// Returns an iterator which encodes action to spawn and yield entities
    /// using bundles yielded from provided bundles iterator.
    #[inline]
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
    #[inline]
    pub fn despawn(&mut self, entity: EntityId) {
        self.push_fn(move |world, buffer| {
            let _ = world.despawn_with_buffer(entity, buffer);
        })
    }

    /// Searches for an entity with specified index.
    #[inline]
    pub fn find_entity(&self, idx: u32) -> Result<EntityId, NoSuchEntity> {
        self.entities.find_entity(idx).ok_or(NoSuchEntity)
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline]
    pub fn insert<T>(&mut self, entity: EntityId, component: T)
    where
        T: Component + Send,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.insert_with_buffer(entity, component, buffer);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline]
    pub fn insert_external<T>(&mut self, entity: EntityId, component: T)
    where
        T: Send + 'static,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.insert_external_with_buffer(entity, component, buffer);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline]
    pub fn insert_bundle<B>(&mut self, entity: EntityId, bundle: B)
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.insert_bundle_with_buffer(entity, bundle, buffer);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline]
    pub fn insert_external_bundle<B>(&mut self, entity: EntityId, bundle: B)
    where
        B: DynamicBundle + Send + 'static,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.insert_external_bundle_with_buffer(entity, bundle, buffer);
        });
    }

    /// Encodes an action to drop component from specified entity.
    #[inline]
    pub fn drop<T>(&mut self, entity: EntityId)
    where
        T: 'static,
    {
        self.drop_erased(entity, TypeId::of::<T>())
    }

    /// Encodes an action to drop component from specified entity.
    #[inline]
    pub fn drop_erased(&mut self, entity: EntityId, ty: TypeId) {
        self.push_fn(move |world, buffer| {
            let _ = world.drop_erased_with_buffer(entity, ty, buffer);
        })
    }

    /// Encodes an action to drop bundle of components from specified entity.
    #[inline]
    pub fn drop_bundle<B>(&mut self, entity: EntityId)
    where
        B: Bundle,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.drop_bundle_with_buffer::<B>(entity, buffer);
        });
    }

    /// Encodes an action to add relation between two entities to the [`World`].
    #[inline]
    pub fn add_relation<R>(&mut self, entity: EntityId, relation: R, target: EntityId)
    where
        R: Relation,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.add_relation_with_buffer(entity, relation, target, buffer);
        });
    }

    /// Encodes an action to drop relation between two entities in the [`World`].
    #[inline]
    pub fn drop_relation<R>(&mut self, entity: EntityId, target: EntityId)
    where
        R: Relation,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.drop_relation_with_buffer::<R>(entity, target, buffer);
        });
    }

    /// Checks if entity is alive.
    #[inline]
    pub fn is_alive(&self, entity: EntityId) -> bool {
        self.entities.get_location(entity).is_some()
    }

    /// Encodes action to insert resource instance.
    pub fn insert_resource<T>(&mut self, resource: T)
    where
        T: Send + 'static,
    {
        self.push_fn(move |world, _| {
            world.insert_resource(resource);
        });
    }

    /// Encodes an action to drop resource instance.
    pub fn drop_resource<T: 'static>(&mut self) {
        self.push_fn(move |world, _| {
            world.remove_resource::<T>();
        });
    }

    /// Encodes a custom action with a closure that takes mutable reference to `World`.
    #[inline]
    pub fn closure(&mut self, fun: impl FnOnce(&mut World) + Send + 'static) {
        self.push_fn(move |world, buffer| world.with_buffer(buffer, fun))
    }

    /// Encodes a custom action with a closure that takes reference to `World`
    /// and another [`ActionEncoder`] that can be used to record new actions.
    #[inline]
    pub fn closure_with_encoder(
        &mut self,
        fun: impl FnOnce(&World, ActionEncoder) + Send + 'static,
    ) {
        self.push_fn(|world, buffer| {
            let encoder = ActionEncoder::new(buffer, world.entity_set());
            fun(world, encoder);
        });
    }

    /// Creates new [`ActionEncoder`] that records actions into the same [`ActionBuffer`]
    /// as this one.
    #[inline]
    pub fn reborrow(&mut self) -> ActionEncoder {
        ActionEncoder {
            actions: self.actions,
            entities: self.entities,
        }
    }

    /// Encodes an action to remove component from specified entity.
    #[inline]
    fn push_fn(&mut self, fun: impl FnOnce(&mut World, &mut ActionBuffer) + Send + 'static) {
        self.actions.push_back(ActionFn::new(fun));
    }
}

/// Buffer with all commands recorded by [`ActionEncoder`].
#[derive(Default)]
#[repr(transparent)]
pub struct ActionBuffer {
    actions: VecDeque<ActionFn<'static>>,
}

impl ActionBuffer {
    /// Returns new empty action buffer.
    pub fn new() -> Self {
        Self {
            actions: VecDeque::new(),
        }
    }

    /// Returns an encoder that records actions into this buffer.
    ///
    /// Actions should be executed on the same [`World`],
    /// otherwise entity ids will not refer to the correct entities.
    pub fn encoder<'a>(&'a mut self, world: &'a World) -> ActionEncoder<'a> {
        ActionEncoder {
            actions: &mut self.actions,
            entities: world.entity_set(),
        }
    }

    /// Executes recorded commands onto the [`World`].
    /// Iterates through all recorded actions and executes them one by one.
    /// After execution buffer is empty.
    ///
    /// Returns `true` if at least one action was executed.
    #[inline]
    pub fn execute(&mut self, world: &mut World) -> bool {
        if self.actions.is_empty() {
            return false;
        }

        while let Some(fun) = self.actions.pop_front() {
            fun.call(world, self);
        }

        true
    }
}

/// Extension trait for slice of [`ActionBuffer`]s.
pub trait ActionBufferSliceExt {
    /// Execute all action encoders from the slice.
    /// Returns `true` if at least one action was executed.
    fn execute_all(&mut self, world: &mut World) -> bool;
}

impl ActionBufferSliceExt for [ActionBuffer] {
    fn execute_all(&mut self, world: &mut World) -> bool {
        self.iter_mut().any(|encoder| encoder.execute(world))
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
    #[inline]
    pub fn spawn_all(self) {
        self.for_each(|_| {});
    }
}

impl<B, I> Iterator for SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle + Send + 'static,
{
    type Item = EntityId;

    #[inline]
    fn next(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next()?;
        Some(self.encoder.spawn_external(bundle))
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<EntityId> {
        // `SpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth(n)?;
        Some(self.encoder.spawn_external(bundle))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    #[inline]
    fn fold<T, F>(mut self, init: T, mut f: F) -> T
    where
        F: FnMut(T, EntityId) -> T,
    {
        let encoder = &mut self.encoder;

        let additional = iter_reserve_hint(&self.bundles);
        encoder.push_fn(move |world, _| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles
            .fold(init, |acc, bundle| f(acc, encoder.spawn_external(bundle)))
    }

    fn collect<T>(mut self) -> T
    where
        T: FromIterator<EntityId>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.

        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world, _| {
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
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<B, I> DoubleEndedIterator for SpawnBatch<'_, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
    fn next_back(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next_back()?;
        Some(self.encoder.spawn_external(bundle))
    }

    fn nth_back(&mut self, n: usize) -> Option<EntityId> {
        // `SpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth_back(n)?;
        Some(self.encoder.spawn_external(bundle))
    }

    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, EntityId) -> T,
    {
        let encoder = &mut self.encoder;

        let additional = iter_reserve_hint(&self.bundles);
        encoder.push_fn(move |world, _| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles
            .rfold(init, |acc, bundle| f(acc, encoder.spawn_external(bundle)))
    }
}

impl<B, I> FusedIterator for SpawnBatch<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
}
