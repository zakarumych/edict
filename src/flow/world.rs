use core::{
    any::type_name,
    future::Future,
    marker::PhantomData,
    mem::MaybeUninit,
    pin::Pin,
    ptr::addr_of_mut,
    task::{Context, Poll},
};

use alloc::sync::Arc;

use crate::{
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle},
    entity::EntityId,
    world::{iter_reserve_hint, World},
    NoSuchEntity,
};

use super::{entity::FlowEntity, flow_world, Flow, NewFlowTask, NewFlows};

/// World reference that is updated when flow is polled.
///
/// It never gives bare references to the world data
/// but provides similar API to work with in flows.
#[derive(Clone, Copy)]
pub struct FlowWorld<'a> {
    pub(super) marker: PhantomData<&'a mut World>,
}

unsafe impl Send for FlowWorld<'_> {}
unsafe impl Sync for FlowWorld<'_> {}

/// Future wrapped to be used as a flow.
#[repr(transparent)]
struct FutureFlow<F> {
    fut: F,
}

impl<F> Flow for FutureFlow<F>
where
    F: Future<Output = ()> + Send + 'static,
{
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = unsafe { self.get_unchecked_mut() };
        let fut = unsafe { Pin::new_unchecked(&mut this.fut) };
        let poll = fut.poll(cx);
        poll
    }
}

/// Trait implemented by functions that can be used to spawn flows.
///
/// First argument represents the enitity itself. It can reference a number of components
/// that are required by the flow.
/// These components will be fetched each time flow is resumed.
/// If non-optional component is missing flow is canceled.
/// Flow declares if it reads or writes into components.
///
/// Second argument is optional and represents the rest of the world.
/// It can be used to access other entities and their components.
pub trait WorldFlowFn<'a> {
    type Fut: Future<Output = ()> + Send + 'a;
    fn run(self, world: FlowWorld<'a>) -> Self::Fut;
}

#[doc(hidden)]
pub fn insert_world_flow<F>(world: &mut World, f: F)
where
    F: WorldFlowFn<'static>,
{
    let mut new_flow_task: NewFlowTask<FutureFlow<F::Fut>> = Arc::new(MaybeUninit::uninit());
    let new_flow_task_mut = Arc::get_mut(&mut new_flow_task).unwrap();

    unsafe {
        let flow_ptr =
            addr_of_mut!((*new_flow_task_mut.as_mut_ptr()).flow).cast::<FutureFlow<F::Fut>>();

        let fut = f.run(FlowWorld::new());
        let fut_ptr = addr_of_mut!((*flow_ptr).fut);
        fut_ptr.write(fut);
    }

    world
        .with_default_resource::<NewFlows>()
        .typed_new_flows()
        .array
        .push(new_flow_task);
}

// Must be callable with any lifetime of `FlowWorld` borrow.
impl<'a, F, Fut> WorldFlowFn<'a> for F
where
    F: FnOnce(FlowWorld<'a>) -> Fut,
    Fut: Future<Output = ()> + Send + 'a,
{
    type Fut = Fut;

    fn run(self, world: FlowWorld<'a>) -> Fut {
        self(world)
    }
}

pub trait WorldFlowSpawn {
    fn spawn(self, world: &mut World);
}

impl<F> WorldFlowSpawn for F
where
    F: for<'a> WorldFlowFn<'a>,
{
    fn spawn(self, world: &mut World) {
        insert_world_flow(world, self);
    }
}

pub struct WorldClosureSpawn<F>(pub F);

impl<F> WorldFlowSpawn for WorldClosureSpawn<F>
where
    F: FnOnce(&mut World),
{
    fn spawn(self, world: &mut World) {
        (self.0)(world)
    }
}

/// Converts closure syntax to flow fn.
///
/// There's limitation that makes following `|world: FlowWorld<'_>| async move { /*use world*/ }`
/// to be noncompilable.
///
/// On nightly it is possible to use `async move |world: FlowWorld<'_>| { /*use world*/ }`
/// But this syntax is not stable yet and edict avoids requiring too many nighty features.
///
/// This macro is a workaround for this limitation.
#[macro_export]
macro_rules! flow_closure {
    (|mut $world:ident $(: $FlowWorld:ty)?| -> $ret:ty $code:block) => {
        $crate::private::WorldClosureSpawn(|world: &mut $crate::world::World| {
            $crate::private::insert_world_flow(
                world,
                |world: $crate::flow::FlowWorld<'static>| async move {
                    let $world $(: $FlowWorld)? = world.reborrow();
                    let res: $ret = { $code };
                    res
                },
            )
        })
    };
    (|mut $world:ident $(: $FlowWorld:ty)?| $code:expr) => {
        $crate::private::WorldClosureSpawn(|world: &mut $crate::world::World| {
            $crate::private::insert_world_flow(
                world,
                |world: $crate::flow::FlowWorld<'static>| async move {
                    let $world $(: $FlowWorld)? = world.reborrow();
                    $code
                },
            )
        })
    };
}

pub fn spawn<F>(world: &mut World, flow_fn: F)
where
    F: WorldFlowSpawn,
{
    flow_fn.spawn(world);
}

pub struct PollWorld<F> {
    f: F,
}

impl<F, R> Future for PollWorld<F>
where
    F: FnMut(&mut World, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            (me.f)(flow_world(), cx)
        }
    }
}

impl<'a> FlowWorld<'a> {
    fn new() -> Self {
        FlowWorld {
            marker: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn reborrow(&self) -> FlowWorld<'_> {
        FlowWorld {
            marker: PhantomData,
        }
    }

    /// Perform operations on the world.
    ///
    /// This is safe since closure cannot yield and refences from world cannot escape.
    pub fn with_sync<R>(&self, f: impl FnOnce(&mut World) -> R) -> R {
        let world = unsafe { flow_world() };
        f(world)
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    pub fn poll_fn<F, R>(&self, f: F) -> PollWorld<F>
    where
        F: FnMut(&mut World, &mut Context) -> Poll<R>,
    {
        PollWorld { f }
    }

    pub fn entity(&self, id: EntityId) -> FlowEntity<'a> {
        FlowEntity::new(id)
    }

    /// Reserves new entity.
    ///
    /// The entity will be materialized before first mutation on the world happens.
    /// Until then entity is alive and belongs to a dummy archetype.
    /// Entity will be alive until despawned.
    ///
    /// # Panics
    ///
    /// Panics if new id cannot be allocated.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let entity = world.allocate().id();
    /// assert!(world.is_alive(entity));
    /// world.despawn(entity).unwrap();
    /// assert!(!world.is_alive(entity));
    /// ```
    #[inline(always)]
    pub fn allocate(&self) -> FlowEntity<'a> {
        let id = unsafe { flow_world().allocate().id() };
        FlowEntity::new(id)
    }

    /// Checks if entity has component of specified type.
    #[inline(always)]
    pub fn has_component<T: 'static>(&self, entity: EntityId) -> bool {
        unsafe { flow_world().try_has_component::<T>(entity).unwrap_or(false) }
    }

    /// Checks if entity is alive.
    #[inline(always)]
    pub fn is_alive(&self, id: EntityId) -> bool {
        unsafe { flow_world().is_alive(id) }
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// Returns [`EntityId`] to the newly spawned entity.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`World::despawn`] is called with returned [`EntityId`].
    ///
    /// # Panics
    ///
    /// Panics if new id cannot be allocated.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent};
    /// let mut world = World::new();
    /// let mut entity = world.spawn((ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline(always)]
    pub fn spawn<B>(&self, bundle: B) -> FlowEntity<'a>
    where
        B: DynamicComponentBundle,
    {
        let id = unsafe { flow_world().spawn(bundle).id() };
        FlowEntity::new(id)
    }

    /// Spawns a new entity in this world with specific ID and bundle of components.
    /// The `World` must be configured to never allocate this ID.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`World::despawn`] is called with the same [`EntityId`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent, EntityId};
    /// let mut world = World::new();
    /// let id = EntityId::from_bits(42).unwrap();
    /// let mut entity = world.spawn_at(id, (ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline(always)]
    pub fn spawn_at<B>(&self, id: EntityId, bundle: B) -> FlowEntity<'a>
    where
        B: DynamicComponentBundle,
    {
        unsafe {
            flow_world().spawn_at(id, bundle);
        }
        FlowEntity::new(id)
    }

    /// Spawns a new entity in this world with specific ID and bundle of components.
    /// The `World` must be configured to never allocate this ID.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`World::despawn`] is called with the same [`EntityId`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent, EntityId};
    /// let mut world = World::new();
    /// let id = EntityId::from_bits(42).unwrap();
    /// let mut entity = world.spawn_or_insert(id, (ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline(always)]
    pub fn spawn_or_insert<B>(&self, id: EntityId, bundle: B) -> FlowEntity<'a>
    where
        B: DynamicComponentBundle,
    {
        unsafe {
            flow_world().spawn_or_insert(id, bundle);
        }
        FlowEntity::new(id)
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// Returns [`EntityRef`] handle to the newly spawned entity.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until despawned.
    ///
    /// All components from the bundle must be previously registered.
    /// If component in bundle implements [`Component`] it could be registered implicitly
    /// on first by [`World::spawn`], [`World::spawn_batch`], [`World::insert`] or [`World::insert_bundle`].
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`] or later by [`World::ensure_component_registered`].
    /// Non [`Component`] types must be pre-registered by [`WorldBuilder::register_external`] or later by [`World::ensure_external_registered`].
    ///
    /// # Panics
    ///
    /// Panics if new id cannot be allocated.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent};
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    /// world.ensure_component_registered::<ExampleComponent>();
    /// let mut entity = world.spawn_external((42u32, ExampleComponent));
    /// assert!(entity.has_component::<u32>());
    /// assert_eq!(entity.remove(), Some(42u32));
    /// assert!(!entity.has_component::<u32>());
    /// ```
    #[inline(always)]
    pub fn spawn_external<B>(&self, bundle: B) -> FlowEntity<'a>
    where
        B: DynamicBundle,
    {
        let id = unsafe { flow_world().spawn_external(bundle).id() };
        FlowEntity::new(id)
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// The id must be unused by the world.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until despawned.
    ///
    /// All components from the bundle must be previously registered.
    /// If component in bundle implements [`Component`] it could be registered implicitly
    /// on first by [`World::spawn`], [`World::spawn_batch`], [`World::insert`] or [`World::insert_bundle`].
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`] or later by [`World::ensure_component_registered`].
    /// Non [`Component`] types must be pre-registered by [`WorldBuilder::register_external`] or later by [`World::ensure_external_registered`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent};
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    /// world.ensure_component_registered::<ExampleComponent>();
    /// let mut entity = world.spawn_external((42u32, ExampleComponent));
    /// assert!(entity.has_component::<u32>());
    /// assert_eq!(entity.remove(), Some(42u32));
    /// assert!(!entity.has_component::<u32>());
    /// ```
    #[inline(always)]
    pub fn spawn_external_at<B>(&self, id: EntityId, bundle: B) -> FlowEntity<'a>
    where
        B: DynamicBundle,
    {
        unsafe {
            flow_world().spawn_external_at(id, bundle);
        }
        FlowEntity::new(id)
    }

    /// Returns an iterator which spawns and yield entities
    /// using bundles yielded from provided bundles iterator.
    ///
    /// When bundles iterator returns `None`, returned iterator returns `None` too.
    ///
    /// If bundles iterator is fused, returned iterator is fused too.
    /// If bundles iterator is double-ended, returned iterator is double-ended too.
    /// If bundles iterator has exact size, returned iterator has exact size too.
    ///
    /// Skipping items on returned iterator will cause bundles iterator skip bundles and not spawn entities.
    ///
    /// Returned iterator attempts to optimize storage allocation for entities
    /// if consumed with functions like `fold`, `rfold`, `for_each` or `collect`.
    ///
    /// When returned iterator is dropped, no more entities will be spawned
    /// even if bundles iterator has items left.
    #[inline(always)]
    pub fn spawn_batch<B, I>(&self, bundles: I) -> SpawnBatch<'a, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: ComponentBundle,
    {
        unsafe {
            flow_world().ensure_bundle_registered::<B>();
        }
        self.spawn_batch_impl(bundles)
    }

    /// Returns an iterator which spawns and yield entities
    /// using bundles yielded from provided bundles iterator.
    ///
    /// When bundles iterator returns `None`, returned iterator returns `None` too.
    ///
    /// If bundles iterator is fused, returned iterator is fused too.
    /// If bundles iterator is double-ended, returned iterator is double-ended too.
    /// If bundles iterator has exact size, returned iterator has exact size too.
    ///
    /// Skipping items on returned iterator will cause bundles iterator skip bundles and not spawn entities.
    ///
    /// Returned iterator attempts to optimize storage allocation for entities
    /// if consumed with functions like `fold`, `rfold`, `for_each` or `collect`.
    ///
    /// When returned iterator is dropped, no more entities will be spawned
    /// even if bundles iterator has items left.
    ///
    /// All components from the bundle must be previously registered.
    /// If component in bundle implements [`Component`] it could be registered implicitly
    /// on first by [`World::spawn`], [`World::spawn_batch`], [`World::insert`] or [`World::insert_bundle`].
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`] or later by [`World::ensure_component_registered`].
    /// Non [`Component`] types must be pre-registered by [`WorldBuilder::register_external`] or later by [`World::ensure_external_registered`].
    #[inline(always)]
    pub fn spawn_batch_external<B, I>(&self, bundles: I) -> SpawnBatch<'a, I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: Bundle,
    {
        self.spawn_batch_impl(bundles)
    }

    fn spawn_batch_impl<B, I>(&self, bundles: I) -> SpawnBatch<'a, I::IntoIter>
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

        SpawnBatch {
            bundles: bundles.into_iter(),
            marker: PhantomData,
        }
    }

    /// Despawns an entity with specified id.
    /// Returns [`Err(NoSuchEntity)`] if entity does not exists.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn((ExampleComponent,)).id();
    /// assert!(world.despawn(entity).is_ok(), "Entity should be despawned by this call");
    /// assert!(world.despawn(entity).is_err(), "Already despawned");
    /// ```
    #[inline(always)]
    pub fn despawn(&self, entity: EntityId) -> Result<(), NoSuchEntity> {
        unsafe { flow_world().despawn(entity) }
    }
}

/// Spawning iterator. Produced by [`World::spawn_batch`].
pub struct SpawnBatch<'a, I> {
    bundles: I,
    marker: PhantomData<&'a mut World>,
}

impl<B, I> SpawnBatch<'_, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    /// Spawns the rest of the entities.
    /// The bundles iterator will be exhausted.
    /// If bundles iterator is fused, calling this method again will
    /// never spawn entities.
    ///
    /// This method won't return IDs of spawned entities.
    #[inline(always)]
    pub fn spawn_all(&mut self) {
        unsafe {
            flow_world()
                .spawn_batch_external(&mut self.bundles)
                .spawn_all();
        }
    }
}

impl<'a, B, I> Iterator for SpawnBatch<'a, I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = FlowEntity<'a>;

    #[inline(always)]
    fn next(&mut self) -> Option<FlowEntity<'a>> {
        let bundle = self.bundles.next()?;
        let id = unsafe { flow_world().spawn_external(bundle).id() };
        Some(FlowEntity::new(id))
    }

    #[inline(always)]
    fn nth(&mut self, n: usize) -> Option<FlowEntity<'a>> {
        // `SpawnBatch` explicitly does NOT spawn entities that are skipped.
        let bundle = self.bundles.nth(n)?;
        let id = unsafe { flow_world().spawn_external(bundle).id() };
        Some(FlowEntity::new(id))
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    #[inline(always)]
    fn fold<T, F>(self, init: T, mut f: F) -> T
    where
        F: FnMut(T, FlowEntity<'a>) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        unsafe {
            let world = flow_world();
            world.spawn_reserve::<B>(additional);
            world
                .spawn_batch_external(self.bundles)
                .fold(init, |acc, e| f(acc, FlowEntity::new(e.id())))
        }
    }

    #[inline(always)]
    fn collect<T>(self) -> T
    where
        T: FromIterator<FlowEntity<'a>>,
    {
        let additional = iter_reserve_hint(&self.bundles);
        unsafe {
            let world = flow_world();
            world.spawn_reserve::<B>(additional);
            world
                .spawn_batch_external(self.bundles)
                .map(|e| FlowEntity::new(e.id()))
                .collect()
        }
    }
}

impl<B, I> ExactSizeIterator for SpawnBatch<'_, I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle,
{
    #[inline(always)]
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<'a, B, I> DoubleEndedIterator for SpawnBatch<'a, I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<FlowEntity<'a>> {
        let bundle = self.bundles.next_back()?;
        let id = unsafe { flow_world().spawn_external(bundle).id() };
        Some(FlowEntity::new(id))
    }

    fn nth_back(&mut self, n: usize) -> Option<FlowEntity<'a>> {
        // No reason to create entities
        // for which the only reference is immediately dropped
        let bundle = self.bundles.nth_back(n)?;
        let id = unsafe { flow_world().spawn_external(bundle).id() };
        Some(FlowEntity::new(id))
    }

    fn rfold<T, F>(self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, FlowEntity<'a>) -> T,
    {
        let additional = iter_reserve_hint(&self.bundles);
        unsafe {
            let world = flow_world();
            world.spawn_reserve::<B>(additional);
            world
                .spawn_batch_external(self.bundles)
                .rfold(init, |acc, e| f(acc, FlowEntity::new(e.id())))
        }
    }
}

impl<B, I> core::iter::FusedIterator for SpawnBatch<'_, I>
where
    I: core::iter::FusedIterator<Item = B>,
    B: Bundle,
{
}
