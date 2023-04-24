use core::{any::TypeId, iter::FusedIterator};

use alloc::collections::VecDeque;

use crate::{
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle},
    component::Component,
    entity::{EntityId, EntitySet},
    relation::Relation,
    world::{iter_reserve_hint, World},
};

use super::{ActionBuffer, ActionFn};

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
    #[inline]
    pub(crate) fn new(buffer: &'a mut ActionBuffer, entities: &'a EntitySet) -> Self {
        ActionEncoder {
            actions: buffer.actions(),
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
    pub fn despawn(&mut self, id: EntityId) {
        self.push_fn(move |world, buffer| {
            let _ = world.despawn_with_buffer(id, buffer);
        })
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline]
    pub fn insert<T>(&mut self, id: EntityId, component: T)
    where
        T: Component + Send,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.insert_with_buffer(id, component, buffer);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline]
    pub fn insert_external<T>(&mut self, id: EntityId, component: T)
    where
        T: Send + 'static,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.insert_external_with_buffer(id, component, buffer);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline]
    pub fn insert_bundle<B>(&mut self, id: EntityId, bundle: B)
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.insert_bundle_with_buffer(id, bundle, buffer);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline]
    pub fn insert_external_bundle<B>(&mut self, id: EntityId, bundle: B)
    where
        B: DynamicBundle + Send + 'static,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.insert_external_bundle_with_buffer(id, bundle, buffer);
        });
    }

    /// Encodes an action to drop component from specified entity.
    #[inline]
    pub fn drop<T>(&mut self, id: EntityId)
    where
        T: 'static,
    {
        self.drop_erased(id, TypeId::of::<T>())
    }

    /// Encodes an action to drop component from specified entity.
    #[inline]
    pub fn drop_erased(&mut self, id: EntityId, ty: TypeId) {
        self.push_fn(move |world, buffer| {
            let _ = world.drop_erased_with_buffer(id, ty, buffer);
        })
    }

    /// Encodes an action to drop bundle of components from specified entity.
    #[inline]
    pub fn drop_bundle<B>(&mut self, id: EntityId)
    where
        B: Bundle,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.drop_bundle_with_buffer::<B>(id, buffer);
        });
    }

    /// Encodes an action to add relation between two entities to the [`World`].
    #[inline]
    pub fn add_relation<R>(&mut self, origin: EntityId, relation: R, target: EntityId)
    where
        R: Relation,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.add_relation_with_buffer(origin, relation, target, buffer);
        });
    }

    /// Encodes an action to drop relation between two entities in the [`World`].
    #[inline]
    pub fn drop_relation<R>(&mut self, origin: EntityId, target: EntityId)
    where
        R: Relation,
    {
        self.push_fn(move |world, buffer| {
            let _ = world.remove_relation_with_buffer::<R>(origin, target, buffer);
        });
    }

    /// Checks if entity is alive.
    #[inline]
    pub fn is_alive(&self, id: EntityId) -> bool {
        self.entities.get_location(id).is_some()
    }

    /// Encodes action to insert resource instance.
    #[inline]
    pub fn insert_resource<T>(&mut self, resource: T)
    where
        T: Send + 'static,
    {
        self.push_fn(move |world, _| {
            world.insert_resource(resource);
        });
    }

    /// Encodes an action to drop resource instance.
    #[inline]
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
        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world, _| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles.fold(init, |acc, bundle| {
            f(acc, self.encoder.spawn_external(bundle))
        })
    }

    #[inline]
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
    #[inline]
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<B, I> DoubleEndedIterator for SpawnBatch<'_, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
    #[inline]
    fn next_back(&mut self) -> Option<EntityId> {
        let bundle = self.bundles.next_back()?;
        Some(self.encoder.spawn_external(bundle))
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<EntityId> {
        // `SpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth_back(n)?;
        Some(self.encoder.spawn_external(bundle))
    }

    #[inline]
    fn rfold<T, F>(mut self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, EntityId) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.encoder.push_fn(move |world, _| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles.rfold(init, |acc, bundle| {
            f(acc, self.encoder.spawn_external(bundle))
        })
    }
}

impl<B, I> FusedIterator for SpawnBatch<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
}
