use alloc::sync::Arc;
use amity::{flip_queue::FlipQueue, ring_buffer::RingBuffer};
use core::{
    any::TypeId,
    iter::FusedIterator,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle},
    component::Component,
    entity::EntityId,
    relation::Relation,
    type_id,
    world::{iter_reserve_hint, World},
};

use super::ActionFn;

struct Shared {
    queue: FlipQueue<ActionFn<'static>>,
    non_empty: AtomicBool,
    connected: AtomicBool,
}

pub(crate) struct ActionChannel {
    shared: Arc<Shared>,
    spare_buffer: RingBuffer<ActionFn<'static>>,
}

impl Drop for ActionChannel {
    fn drop(&mut self) {
        self.shared.connected.store(false, Ordering::Relaxed);
    }
}

impl ActionChannel {
    #[inline]
    pub fn new() -> Self {
        ActionChannel {
            shared: Arc::new(Shared {
                queue: FlipQueue::new(),
                non_empty: AtomicBool::new(false),
                connected: AtomicBool::new(true),
            }),
            spare_buffer: RingBuffer::new(),
        }
    }

    #[inline]
    pub fn sender(&self) -> ActionSender {
        ActionSender {
            shared: self.shared.clone(),
        }
    }

    /// Fetches actions recorded into the channel.
    #[inline]
    pub fn fetch(&mut self) {
        debug_assert!(self.spare_buffer.is_empty());
        if self.shared.non_empty.swap(false, Ordering::Relaxed) {
            self.shared.queue.swap_buffer(&mut self.spare_buffer);
        }
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<ActionFn<'static>> {
        self.spare_buffer.pop()
    }
}

/// A channel for encoding actions and sending to the [`World`] thread-safely.
///
/// Use this when actions need to be encoded not in a system
/// but in a thread separate from ECS executor or in async task.
///
/// The API is similar to [`crate::action::ActionEncoder`], but entity allocation is not supported
/// and [`spawn`](ActionSender::spawn) and other methods do not return [`EntityId`]s.
///
/// Unlike [`crate::action::ActionBuffer`], the channel is bound to a specific [`World`] instance.
/// If bound [`World`] is dropped, encoded actions will not be executed.
/// See [`ActionSender::is_connected`](ActionSender::is_connected) to check
/// if the channel is still connected to a world.
#[derive(Clone)]
pub struct ActionSender {
    shared: Arc<Shared>,
}

impl ActionSender {
    /// Encodes an action to spawn an entity with provided bundle.
    #[inline]
    pub fn spawn<B>(&self, bundle: B)
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        self.push_fn(move |world| {
            let _ = world.spawn(bundle);
        });
    }

    /// Encodes an action to spawn an entity with provided bundle.
    #[inline]
    pub fn spawn_external<B>(&self, bundle: B)
    where
        B: DynamicBundle + Send + 'static,
    {
        self.push_fn(move |world| {
            let _ = world.spawn_external(bundle);
        });
    }

    /// Returns an iterator which encodes action to spawn entities
    /// using bundles yielded from provided bundles iterator.
    #[inline]
    pub fn spawn_batch<I>(&self, bundles: I) -> SpawnBatchSender<I>
    where
        I: IntoIterator,
        I::Item: ComponentBundle + Send + 'static,
    {
        self.push_fn(|world| {
            world.ensure_bundle_registered::<I::Item>();
        });

        SpawnBatchSender {
            bundles,
            sender: self,
        }
    }

    /// Returns an iterator which encodes action to spawn entities
    /// using bundles yielded from provided bundles iterator.
    #[inline]
    pub fn spawn_external_batch<I>(&self, bundles: I) -> SpawnBatchSender<I>
    where
        I: IntoIterator,
        I::Item: Bundle + Send + 'static,
    {
        SpawnBatchSender {
            bundles,
            sender: self,
        }
    }

    /// Encodes an action to despawn specified entity.
    #[inline]
    pub fn despawn(&self, id: EntityId) {
        self.push_fn(move |world| {
            let _ = world.despawn(id);
        })
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline]
    pub fn insert<T>(&self, id: EntityId, component: T)
    where
        T: Component + Send,
    {
        self.push_fn(move |world| {
            let _ = world.insert(id, component);
        });
    }

    /// Encodes an action to insert component to the specified entity.
    #[inline]
    pub fn insert_external<T>(&self, id: EntityId, component: T)
    where
        T: Send + 'static,
    {
        self.push_fn(move |world| {
            let _ = world.insert_external(id, component);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline]
    pub fn insert_bundle<B>(&self, id: EntityId, bundle: B)
    where
        B: DynamicComponentBundle + Send + 'static,
    {
        self.push_fn(move |world| {
            let _ = world.insert_bundle(id, bundle);
        });
    }

    /// Encodes an action to insert components from bundle to the specified entity.
    #[inline]
    pub fn insert_external_bundle<B>(&self, id: EntityId, bundle: B)
    where
        B: DynamicBundle + Send + 'static,
    {
        self.push_fn(move |world| {
            let _ = world.insert_external_bundle(id, bundle);
        });
    }

    /// Encodes an action to drop component from specified entity.
    #[inline]
    pub fn drop<T>(&self, id: EntityId)
    where
        T: 'static,
    {
        self.drop_erased(id, type_id::<T>())
    }

    /// Encodes an action to drop component from specified entity.
    #[inline]
    pub fn drop_erased(&self, id: EntityId, ty: TypeId) {
        self.push_fn(move |world| {
            let _ = world.drop_erased(id, ty);
        })
    }

    /// Encodes an action to drop bundle of components from specified entity.
    #[inline]
    pub fn drop_bundle<B>(&self, id: EntityId)
    where
        B: Bundle,
    {
        self.push_fn(move |world| {
            let _ = world.drop_bundle::<B>(id);
        });
    }

    /// Encodes an action to add relation between two entities to the [`World`].
    #[inline]
    pub fn insert_relation<R>(&self, origin: EntityId, relation: R, target: EntityId)
    where
        R: Relation + Send,
    {
        self.push_fn(move |world| {
            let _ = world.insert_relation(origin, relation, target);
        });
    }

    /// Encodes an action to drop relation between two entities in the [`World`].
    #[inline]
    pub fn drop_relation<R>(&self, origin: EntityId, target: EntityId)
    where
        R: Relation,
    {
        self.push_fn(move |world| {
            let _ = world.remove_relation::<R>(origin, target);
        });
    }

    /// Encodes action to insert resource instance.
    pub fn insert_resource<T>(&self, resource: T)
    where
        T: Send + 'static,
    {
        self.push_fn(move |world| {
            world.insert_resource(resource);
        });
    }

    /// Encodes an action to drop resource instance.
    pub fn drop_resource<T: 'static>(&self) {
        self.push_fn(move |world| {
            world.remove_resource::<T>();
        });
    }

    /// Encodes a custom action with a closure that takes mutable reference to `World`.
    #[inline]
    pub fn closure(&self, fun: impl FnOnce(&mut World) + Send + 'static) {
        self.push_fn(fun)
    }

    /// Encodes an action to remove component from specified entity.
    #[inline]
    fn push_fn(&self, fun: impl FnOnce(&mut World) + Send + 'static) {
        let action = ActionFn::new(fun);
        self.shared.queue.push_sync(action);
        self.shared.non_empty.store(true, Ordering::Relaxed);
    }

    /// Returns `true` if the channel is still connected to a [`World`] instance.
    #[inline]
    pub fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Relaxed)
    }
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
pub struct SpawnBatchSender<'a, I> {
    bundles: I,
    sender: &'a ActionSender,
}

impl<B, I> SpawnBatchSender<'_, I>
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

impl<B, I> Iterator for SpawnBatchSender<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle + Send + 'static,
{
    type Item = ();

    #[inline]
    fn next(&mut self) -> Option<()> {
        let bundle = self.bundles.next()?;
        Some(self.sender.spawn_external(bundle))
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<()> {
        // `SpawnBatchSender` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth(n)?;
        Some(self.sender.spawn_external(bundle))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    #[inline]
    fn fold<T, F>(self, init: T, mut f: F) -> T
    where
        F: FnMut(T, ()) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.sender.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles.fold(init, |acc, bundle| {
            f(acc, self.sender.spawn_external(bundle))
        })
    }

    #[inline]
    fn collect<T>(self) -> T
    where
        T: FromIterator<()>,
    {
        // `FromIterator::from_iter` would probably just call `fn next()`
        // until the end of the iterator.
        //
        // Hence we should reserve space in archetype here.

        let additional = iter_reserve_hint(&self.bundles);
        self.sender.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        FromIterator::from_iter(self)
    }
}

impl<B, I> ExactSizeIterator for SpawnBatchSender<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle + Send + 'static,
{
    #[inline]
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<B, I> DoubleEndedIterator for SpawnBatchSender<'_, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
    #[inline]
    fn next_back(&mut self) -> Option<()> {
        let bundle = self.bundles.next_back()?;
        Some(self.sender.spawn_external(bundle))
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<()> {
        // `SpawnBatchSender` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth_back(n)?;
        Some(self.sender.spawn_external(bundle))
    }

    #[inline]
    fn rfold<T, F>(self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, ()) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        self.sender.push_fn(move |world| {
            world.spawn_reserve::<B>(additional);
        });

        self.bundles.rfold(init, |acc, bundle| {
            f(acc, self.sender.spawn_external(bundle))
        })
    }
}

impl<B, I> FusedIterator for SpawnBatchSender<'_, I>
where
    I: FusedIterator<Item = B>,
    B: Bundle + Send + 'static,
{
}
