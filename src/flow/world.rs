use core::{
    any::TypeId,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    bundle::{Bundle, ComponentBundle, DynamicBundle, DynamicComponentBundle},
    component::Component,
    entity::{Entity, EntityId},
    query::{DefaultQuery, IntoQuery, Query},
    relation::Relation,
    view::View,
    world::WorldLocal,
    EntityError, NoSuchEntity,
};

use super::{get_flow_world, Flow, FlowEntity};

/// Type that can be spawned as a flow.
/// It can be an async function or a closure
/// that accepts [`FlowWorld`] as the only argument.
///
/// # Example
///
/// ```
/// # use edict::flow::{self, FlowWorld};
///
/// let mut world = edict::world::World::new();
///
/// world.spawn_flow(|world: FlowWorld| async move {
///   let entity = world.spawn(());
/// });
/// ```
#[diagnostic::on_unimplemented(
    note = "Try `async fn(world: FlowWorld)` or `|world: FlowWorld| async {{ ... }}`"
)]
pub trait IntoFlow: 'static {
    /// Flow type that will be polled.
    type Flow: Flow;

    /// Converts self into a flow.
    fn into_flow(self, world: FlowWorld) -> Option<Self::Flow>;
}

/// World reference that is updated when flow is polled.
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct FlowWorld {
    marker: PhantomData<fn() -> &'static mut WorldLocal>,
}

/// Future wrapped to be used as a flow.
#[repr(transparent)]
#[doc(hidden)]
pub struct FutureFlow<F> {
    fut: F,
}

impl<F> Flow for FutureFlow<F>
where
    F: Future<Output = ()> + Send,
{
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = unsafe { self.get_unchecked_mut() };
        let fut = unsafe { Pin::new_unchecked(&mut this.fut) };
        let poll = fut.poll(cx);
        poll
    }
}

impl<F, Fut> IntoFlow for F
where
    F: FnOnce(FlowWorld) -> Fut + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Flow = FutureFlow<Fut>;

    fn into_flow(self, world: FlowWorld) -> Option<Self::Flow> {
        Some(FutureFlow { fut: self(world) })
    }
}

impl FlowWorld {
    #[inline]
    pub(super) fn new() -> Self {
        FlowWorld {
            marker: PhantomData,
        }
    }

    /// Returns reference to currently bound world.
    ///
    /// It is easy to accidentally create aliasing references to the world.
    /// Prefer to use safe methods provided on this type to interact with the world.
    ///
    /// If access to `WorldLocal` is required, consider using [`FlowWorld::poll`] first.
    ///
    /// # Safety
    ///
    /// Creates reference from raw pointer to `WorldLocal` bound to the flow context.
    /// Returned reference is unbound so it may outlive referenced world and become dangling.
    /// Creating more than one mutable reference to the world may also cause undefined behavior.
    ///
    /// Note that many methods of [`FlowWorld`] or [`FlowEntity`]
    /// create temporary mutable references to the world.
    ///
    /// So calling them while this reference is alive is not allowed.
    pub unsafe fn get<'a>(self) -> &'a mut WorldLocal {
        unsafe { get_flow_world() }
    }

    /// Access world reference with closure.
    /// Returns closure result.
    ///
    /// Unlike [`FlowWorld::get`] this method is safe as it does not allow references to world to escape closure.
    /// And therefore references won't able to outlive await boundaries.
    ///
    /// Use [`FlowWorld::poll`] if you need to call poll closure until certain condition is met.
    pub fn map<F, R>(self, f: F) -> R
    where
        F: FnOnce(&mut WorldLocal) -> R,
    {
        f(unsafe { self.get() })
    }

    /// Returns a future that will poll the closure with world reference.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    #[inline]
    pub fn poll<F, R>(self, f: F) -> PollWorld<F>
    where
        F: FnMut(&mut WorldLocal, &mut Context) -> Poll<R>,
    {
        PollWorld { f, world: self }
    }

    /// Returns a future that will poll the closure with world view
    /// constructed with specified query type.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    pub fn poll_view<Q, F, R>(self, f: F) -> PollView<Q::Query, (), F>
    where
        Q: DefaultQuery,
        F: FnMut(View<Q>, &mut Context) -> Poll<R>,
    {
        PollView {
            f,
            query: Q::default_query(),
            filter: (),
            world: self,
        }
    }

    /// Returns a future that will poll the closure with world view
    /// constructed with specified query type and filter type.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    pub fn poll_view_filter<Q, F, Fun, R>(self, f: Fun) -> PollView<Q::Query, F::Query, Fun>
    where
        Q: DefaultQuery,
        F: DefaultQuery,
        F: FnMut(View<Q, F>, &mut Context) -> Poll<R>,
    {
        PollView {
            f,
            query: Q::default_query(),
            filter: F::default_query(),
            world: self,
        }
    }

    /// Returns a future that will poll the closure with world view
    /// constructed with specified query.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    pub fn poll_view_with<Q, F, R>(self, query: Q, f: F) -> PollView<Q::Query, (), F>
    where
        Q: IntoQuery,
        F: FnMut(View<Q>, &mut Context) -> Poll<R>,
    {
        PollView {
            f,
            query: query.into_query(),
            filter: (),
            world: self,
        }
    }

    /// Returns a future that will poll the closure with world view
    /// constructed with specified query and filter.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    pub fn poll_view_filter_with<Q, F, Fun, R>(
        self,
        query: Q,
        filter: F,
        f: Fun,
    ) -> PollView<Q::Query, F::Query, Fun>
    where
        Q: IntoQuery,
        F: IntoQuery,
        Fun: FnMut(View<Q, F>, &mut Context) -> Poll<R>,
    {
        PollView {
            f,
            query: query.into_query(),
            filter: filter.into_query(),
            world: self,
        }
    }

    /// Returns entity reference.
    /// Returns [`NoSuchEntity`] error if entity is not alive.
    #[inline]
    pub fn entity(self, entity: EntityId) -> Result<FlowEntity, NoSuchEntity> {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        if world.is_alive(entity) {
            Ok(FlowEntity::new(entity))
        } else {
            Err(NoSuchEntity)
        }
    }

    /// Queries components from specified entity.
    /// Where query item is a reference to value the implements [`ToOwned`].
    /// Returns item converted to owned value.
    ///
    /// This method locks only archetype to which entity belongs for the duration of the method itself.
    #[inline]
    pub fn try_get_cloned<T>(self, entity: impl Entity) -> Result<T, EntityError>
    where
        T: Clone + 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.try_get_cloned(entity)
    }

    /// Attempts to inserts component to the specified entity.
    ///
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
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert(entity, ExampleComponent).unwrap();
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert<T>(self, entity: impl Entity, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.insert(entity, component)
    }

    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(false));
    /// world.ensure_external_registered::<u32>();
    /// world.insert_external(entity, 42u32).unwrap();
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert_external<T>(self, entity: impl Entity, component: T) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.insert_external(entity, component)
    }

    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is preserved.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// world.with(entity, || ExampleComponent).unwrap();
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn with<T>(self, entity: impl Entity, f: impl FnOnce() -> T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.with(entity, f)?;

        Ok(())
    }

    /// Attempts to inserts component to the specified entity.
    ///
    /// If entity already had component of that type,
    /// old component value is preserved.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(false));
    /// world.ensure_external_registered::<u32>();
    /// world.with_external(entity, || 42u32).unwrap();
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn with_external<T>(
        self,
        entity: impl Entity,
        f: impl FnOnce() -> T,
    ) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.with_external(entity, f)?;

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
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert_bundle(entity, (ExampleComponent,));
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert_bundle<B>(self, entity: impl Entity, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.insert_bundle(entity, bundle)
    }

    /// Inserts bundle of components to the specified entity.
    /// This is moral equivalent to calling `World::insert` with each component separately,
    /// but more efficient.
    ///
    /// If entity has component of any type in bundle already,
    /// it is replaced with new one.
    /// Otherwise components is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(false));
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// world.insert_external_bundle(entity, (ExampleComponent, 42u32));
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn insert_external_bundle<B>(
        self,
        entity: impl Entity,
        bundle: B,
    ) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.insert_external_bundle(entity, bundle)
    }

    /// Inserts bundle of components to the specified entity.
    /// Adds only components missing from the entity.
    /// Components that are already present are not replaced,
    /// if replacing is required use [`FlowWorld::insert_bundle`].
    ///
    /// This function guarantees that no hooks are triggered,
    /// and entity cannot be despawned as a result of this operation.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// world.insert_bundle(entity, (ExampleComponent,));
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn with_bundle<B>(self, entity: impl Entity, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.with_bundle(entity, bundle)?;

        Ok(())
    }

    /// Inserts bundle of components to the specified entity.
    /// Adds only components missing from the entity.
    /// Components that are already present are not replaced,
    /// if replacing is required use [`FlowWorld::insert_bundle`].
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn(()).id();
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(false));
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(false));
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// world.insert_external_bundle(entity, (ExampleComponent, 42u32));
    ///
    /// assert_eq!(world.try_has_component::<ExampleComponent>(entity), Ok(true));
    /// assert_eq!(world.try_has_component::<u32>(entity), Ok(true));
    /// ```
    #[inline]
    pub fn with_external_bundle<B>(self, entity: impl Entity, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.with_external_bundle(entity, bundle)?;

        Ok(())
    }

    /// Removes component from the specified entity and returns its value.
    ///
    /// Returns `Ok(Some(comp))` if component was removed.
    /// Returns `Ok(None)` if entity does not have component of this type.
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    #[inline]
    pub fn remove<T>(self, entity: impl Entity) -> Result<Option<T>, NoSuchEntity>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        let (c, _) = world.remove(entity)?;
        Ok(c)
    }

    /// Drops component from the specified entity.
    ///
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    #[inline]
    pub fn drop<T>(self, entity: impl Entity) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.drop::<T>(entity)
    }

    /// Drops component from the specified entity.
    ///
    /// Returns `Err(NoSuchEntity)` if entity is not alive.
    #[inline]
    pub fn drop_erased(self, entity: impl Entity, ty: TypeId) -> Result<(), NoSuchEntity> {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.drop_erased(entity, ty)
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
    /// let mut entity = world.spawn((ExampleComponent,)).id();
    ///
    /// assert!(world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// world.drop_bundle::<(ExampleComponent, OtherComponent)>(entity).unwrap();
    /// assert!(!world.try_has_component::<ExampleComponent>(entity).unwrap());
    /// ```
    #[inline]
    pub fn drop_bundle<B>(self, entity: impl Entity) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.drop_bundle::<B>(entity)
    }

    /// Adds relation between two entities to the [`FlowWorld`].
    ///
    /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    /// When either entity is despawned, relation is removed automatically.
    ///
    /// Relations can be queried and filtered using queries from [`edict::relation`] module.
    ///
    /// Relation must implement [`Relation`] trait that defines its behavior.
    ///
    /// If relation already exists, then instance is replaced.
    /// If relation is symmetric then it is added in both directions.
    /// If relation is exclusive, then previous relation on origin is replaced, otherwise relation is added.
    /// If relation is exclusive and symmetric, then previous relation on target is replaced, otherwise relation is added.
    #[inline]
    pub fn insert_relation<R>(
        self,
        origin: impl Entity,
        relation: R,
        target: impl Entity,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.insert_relation(origin, relation, target)
    }

    /// Removes relation between two entities in the [`FlowWorld`].
    ///
    /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    /// If relation does not exist, removes `None`.
    ///
    /// When relation is removed, [`Relation::on_drop`] behavior is not executed.
    /// For symmetric relations [`Relation::on_target_drop`] is also not executed.
    #[inline]
    pub fn remove_relation<R>(
        self,
        origin: impl Entity,
        target: impl Entity,
    ) -> Result<Option<R>, NoSuchEntity>
    where
        R: Relation,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.remove_relation(origin, target)
    }

    /// Drops relation between two entities in the [`FlowWorld`].
    ///
    /// If either entity is not alive, fails with `Err(NoSuchEntity)`.
    /// If relation does not exist, does nothing.
    ///
    /// When relation is dropped, [`Relation::on_drop`] behavior is executed.
    #[inline]
    pub fn drop_relation<R>(
        self,
        origin: impl Entity,
        target: impl Entity,
    ) -> Result<(), NoSuchEntity>
    where
        R: Relation,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.drop_relation::<R>(origin, target)
    }

    /// Inserts resource instance.
    /// Old value is replaced.
    ///
    /// To access resource, use [`FlowWorld::get_resource`] and [`FlowWorld::get_resource_mut`] methods.
    ///
    /// [`FlowWorld::get_resource`]: struct.World.html#method.get_resource
    /// [`FlowWorld::get_resource_mut`]: struct.World.html#method.get_resource_mut
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// *world.get_resource_mut::<i32>().unwrap() = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn insert_resource<T: 'static>(self, resource: T) {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.insert_resource(resource);
    }

    /// Returns reference to the resource instance.
    /// Inserts new instance if it does not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let value = world.with_resource(|| 42i32);
    /// assert_eq!(*value, 42);
    /// *value = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn with_resource<T: 'static>(self, f: impl FnOnce() -> T) {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.with_resource(f);
    }

    /// Returns reference to the resource instance.
    /// Inserts new instance if it does not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let value = world.with_default_resource::<u32>();
    /// assert_eq!(*value, 0);
    /// *value = 11;
    /// assert_eq!(*world.get_resource::<u32>().unwrap(), 11);
    /// ```
    pub fn with_default_resource<T: Default + 'static>(self) {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.with_default_resource::<T>();
    }

    /// Remove resource instance.
    /// Returns `None` if resource was not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// world.remove_resource::<i32>();
    /// assert!(world.get_resource::<i32>().is_none());
    /// ```
    pub fn remove_resource<T: 'static>(self) -> Option<T> {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.remove_resource()
    }

    /// Returns a copy for the `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.copy_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(world.copy_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn copy_resource<T: Copy + 'static>(self) -> T {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.copy_resource()
    }

    /// Returns a clone for the `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.copy_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(world.copy_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn clone_resource<T: Clone + 'static>(self) -> T {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.clone_resource()
    }

    /// Spawns a new entity in this world without components.
    /// Returns [`FlowEntity`] for the newly spawned entity.
    /// Entity will be alive until [`FlowWorld::despawn`] is called with [`EntityId`] of the spawned entity,
    /// or despawn command recorded and executed by the [`FlowWorld`].
    ///
    /// # Panics
    ///
    /// If new id cannot be allocated.
    /// If too many entities are spawned.
    /// Currently limit is set to `u32::MAX` entities per archetype and `usize::MAX` overall.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let mut entity = world.spawn_empty();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline]
    pub fn spawn_empty(self) -> FlowEntity {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        FlowEntity::new(world.spawn_empty().id())
    }

    /// Spawns a new entity in this world with provided component.
    /// Returns [`FlowEntity`] for the newly spawned entity.
    /// Entity will be alive until [`FlowWorld::despawn`] is called with [`EntityId`] of the spawned entity,
    /// or despawn command recorded and executed by the [`FlowWorld`].
    ///
    /// # Panics
    ///
    /// If new id cannot be allocated.
    /// If too many entities are spawned.
    /// Currently limit is set to `u32::MAX` entities per archetype and `usize::MAX` overall.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let mut entity = world.spawn_one(ExampleComponent);
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline]
    pub fn spawn_one<T>(self, component: T) -> FlowEntity
    where
        T: Component,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        FlowEntity::new(world.spawn_one(component).id())
    }

    /// Spawns a new entity in this world with provided component.
    /// Returns [`FlowEntity`] for the newly spawned entity.
    /// Entity will be alive until [`FlowWorld::despawn`] is called with [`EntityId`] of the spawned entity,
    /// or despawn command recorded and executed by the [`FlowWorld`].
    ///
    /// Component must be previously registered.
    /// If component implements [`Component`] it could be registered implicitly
    /// on first call to [`FlowWorld::spawn`], [`FlowWorld::spawn_one`],  [`FlowWorld::spawn_batch`], [`FlowWorld::insert`] or [`FlowWorld::insert_bundle`] and their deferred versions.
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`](crate::world::WorldBuilder::register_component) or later by [`FlowWorld::ensure_component_registered`].
    /// Non [`Component`] type must be pre-registered by [`WorldBuilder::register_external`](crate::world::WorldBuilder::register_external) or later by [`FlowWorld::ensure_external_registered`].
    ///
    /// # Panics
    ///
    /// If new id cannot be allocated.
    /// If too many entities are spawned.
    /// Currently limit is set to `u32::MAX` entities per archetype and `usize::MAX` overall.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    /// let mut entity = world.spawn_one_external(42u32);
    /// assert!(entity.has_component::<u32>());
    /// ```
    #[inline]
    pub fn spawn_one_external<T>(self, component: T) -> FlowEntity
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        FlowEntity::new(world.spawn_one_external(component).id())
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// Returns [`FlowEntity`] for the newly spawned entity.
    /// Entity will be alive until [`FlowWorld::despawn`] is called with [`EntityId`] of the spawned entity,
    /// or despawn command recorded and executed by the [`FlowWorld`].
    ///
    /// # Panics
    ///
    /// If new id cannot be allocated.
    /// If too many entities are spawned.
    /// Currently limit is set to `u32::MAX` entities per archetype and `usize::MAX` overall.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let mut entity = world.spawn((ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline]
    pub fn spawn<B>(self, bundle: B) -> FlowEntity
    where
        B: DynamicComponentBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        FlowEntity::new(world.spawn(bundle).id())
    }

    /// Spawns a new entity in this world with specific ID and bundle of components.
    /// The `World` must be configured to never allocate this ID.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`FlowWorld::despawn`] is called with the same [`EntityId`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, entity::EntityId, ExampleComponent};
    /// let mut world = World::new();
    /// let id = EntityId::from_bits(42).unwrap();
    /// let mut entity = world.spawn_at(id, (ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline]
    pub fn spawn_at<B>(self, id: EntityId, bundle: B) -> FlowEntity
    where
        B: DynamicComponentBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        FlowEntity::new(world.spawn_at(id, bundle).id())
    }

    /// Spawns a new entity in this world with specific ID and bundle of components.
    /// The `World` must be configured to never allocate this ID.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until [`FlowWorld::despawn`] is called with the same [`EntityId`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, entity::EntityId, ExampleComponent};
    /// let mut world = World::new();
    /// let id = EntityId::from_bits(42).unwrap();
    /// let mut entity = world.spawn_or_insert(id, (ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// let ExampleComponent = entity.remove().unwrap();
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
    #[inline]
    pub fn spawn_or_insert<B>(self, id: EntityId, bundle: B) -> FlowEntity
    where
        B: DynamicComponentBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        FlowEntity::new(world.spawn_or_insert(id, bundle).id())
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// Returns [`FlowEntity`] handle to the newly spawned entity.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until despawned.
    ///
    /// Components must be previously registered.
    /// If component implements [`Component`] it could be registered implicitly
    /// on first call to [`FlowWorld::spawn`], [`FlowWorld::spawn_one`],  [`FlowWorld::spawn_batch`], [`FlowWorld::insert`] or [`FlowWorld::insert_bundle`] and their deferred versions.
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`](crate::world::WorldBuilder::register_component) or later by [`FlowWorld::ensure_component_registered`].
    /// Non [`Component`] type must be pre-registered by [`WorldBuilder::register_external`](crate::world::WorldBuilder::register_external) or later by [`FlowWorld::ensure_external_registered`].
    ///
    /// # Panics
    ///
    /// Panics if new id cannot be allocated.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    /// world.ensure_component_registered::<ExampleComponent>();
    /// let mut entity = world.spawn_external((42u32, ExampleComponent));
    /// assert!(entity.has_component::<u32>());
    /// assert_eq!(entity.remove(), Some(42u32));
    /// assert!(!entity.has_component::<u32>());
    /// ```
    #[inline]
    pub fn spawn_external<B>(self, bundle: B) -> FlowEntity
    where
        B: DynamicBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        FlowEntity::new(world.spawn_external(bundle).id())
    }

    /// Spawns a new entity in this world with provided bundle of components.
    /// The id must be unused by the world.
    /// Spawned entity is populated with all components from the bundle.
    /// Entity will be alive until despawned.
    ///
    /// Components must be previously registered.
    /// If component implements [`Component`] it could be registered implicitly
    /// on first call to [`FlowWorld::spawn`], [`FlowWorld::spawn_one`],  [`FlowWorld::spawn_batch`], [`FlowWorld::insert`] or [`FlowWorld::insert_bundle`] and their deferred versions.
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`](crate::world::WorldBuilder::register_component) or later by [`FlowWorld::ensure_component_registered`].
    /// Non [`Component`] type must be pre-registered by [`WorldBuilder::register_external`](crate::world::WorldBuilder::register_external) or later by [`FlowWorld::ensure_external_registered`].
    ///
    /// # Panics
    ///
    /// Panics if the id is already used by the world.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    /// world.ensure_component_registered::<ExampleComponent>();
    /// let mut entity = world.spawn_external((42u32, ExampleComponent));
    /// assert!(entity.has_component::<u32>());
    /// assert_eq!(entity.remove(), Some(42u32));
    /// assert!(!entity.has_component::<u32>());
    /// ```
    #[inline]
    pub fn spawn_external_at<B>(self, id: EntityId, bundle: B) -> FlowEntity
    where
        B: DynamicBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        FlowEntity::new(world.spawn_external_at(id, bundle).id())
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
    #[inline]
    pub fn spawn_batch<B, I>(self, bundles: I) -> SpawnBatch<I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: ComponentBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.ensure_bundle_registered::<B>();

        SpawnBatch {
            bundles: bundles.into_iter(),
            world: self,
        }
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
    /// Components must be previously registered.
    /// If component implements [`Component`] it could be registered implicitly
    /// on first call to [`FlowWorld::spawn`], [`FlowWorld::spawn_one`],  [`FlowWorld::spawn_batch`], [`FlowWorld::insert`] or [`FlowWorld::insert_bundle`] and their deferred versions.
    /// Otherwise component must be pre-registered explicitly by [`WorldBuilder::register_component`](crate::world::WorldBuilder::register_component) or later by [`FlowWorld::ensure_component_registered`].
    /// Non [`Component`] type must be pre-registered by [`WorldBuilder::register_external`](crate::world::WorldBuilder::register_external) or later by [`FlowWorld::ensure_external_registered`].
    #[inline]
    pub fn spawn_batch_external<B, I>(self, bundles: I) -> SpawnBatch<I::IntoIter>
    where
        I: IntoIterator<Item = B>,
        B: Bundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.assert_bundle_registered::<B>();

        SpawnBatch {
            bundles: bundles.into_iter(),
            world: self,
        }
    }

    /// Despawns an entity with specified id.
    /// Returns [`Err(NoSuchEntity)`] if entity does not exists.
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let entity = world.spawn((ExampleComponent,)).id();
    /// assert!(world.despawn(entity).is_ok(), "Entity should be despawned by this call");
    /// assert!(world.despawn(entity).is_err(), "Already despawned");
    /// ```
    #[inline]
    pub fn despawn(self, entity: impl Entity) -> Result<(), NoSuchEntity> {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.despawn(entity)
    }

    /// Explicitly registers component type.
    ///
    /// Unlike [`WorldBuilder::register_component`](crate::world::WorldBuilder::register_component) method, this method does not return reference to component configuration,
    /// once [`FlowWorld`] is created overriding component behavior is not possible.
    ///
    /// Component types are implicitly registered on first use by most methods.
    /// This method is only needed if you want to use component type using
    /// [`FlowWorld::insert_external`], [`FlowWorld::insert_external_bundle`] or [`FlowWorld::spawn_external`].
    pub fn ensure_component_registered<T>(self)
    where
        T: Component,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.ensure_component_registered::<T>();
    }

    /// Explicitly registers external type.
    ///
    /// Unlike [`WorldBuilder::register_external`](crate::world::WorldBuilder::register_external) method, this method does not return reference to component configuration,
    /// once [`FlowWorld`] is created overriding component behavior is not possible.
    ///
    /// External component types are not implicitly registered on first use.
    /// This method is needed if you want to use component type with
    /// [`FlowWorld::insert_external`], [`FlowWorld::insert_external_bundle`] or [`FlowWorld::spawn_external`].
    pub fn ensure_external_registered<T>(&mut self)
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.ensure_external_registered::<T>();
    }

    /// Explicitly registers bundle of component types.
    ///
    /// This method is only needed if you want to use bundle of component types using
    /// [`FlowWorld::insert_external_bundle`] or [`FlowWorld::spawn_external`].
    pub fn ensure_bundle_registered<B>(&mut self)
    where
        B: ComponentBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.ensure_bundle_registered::<B>();
    }

    /// Asserts that all components from the bundle are registered.
    pub fn assert_bundle_registered<B>(&self)
    where
        B: Bundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { self.get() };

        world.assert_bundle_registered::<B>();
    }
}

/// Future that polls the closure with world reference.
///
/// Resolves to closure result in [`Poll::Ready`].
/// The closure may use task context to register wakers.
#[must_use = "Future does nothing unless polled"]
pub struct PollWorld<F> {
    f: F,
    world: FlowWorld,
}

impl<F, R> Future for PollWorld<F>
where
    F: FnMut(&mut WorldLocal, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        // Safety: `me` never moves.
        let me = unsafe { self.get_unchecked_mut() };

        // Safety: world reference does not escape this scope.
        let world = unsafe { me.world.get() };

        (me.f)(world, cx)
    }
}

/// Future that polls the closure with world view
/// constructed with specified query.
///
/// Resolves to closure result in [`Poll::Ready`].
/// The closure may use task context to register wakers.
#[must_use = "Future does nothing unless polled"]
pub struct PollView<Q, F, Fun> {
    query: Q,
    filter: F,
    f: Fun,
    world: FlowWorld,
}

impl<Q, F, Fun, R> Future for PollView<Q, F, Fun>
where
    Q: Query,
    F: Query,
    for<'a> Fun: FnMut(View<'a, Q, F>, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        // Safety: `me` never moves.
        let me = unsafe { self.get_unchecked_mut() };

        // Safety: world reference does not escape this scope.
        let world = unsafe { me.world.get() };

        let view = world.view_filter_with_mut(me.query, me.filter);
        (me.f)(view.into(), cx)
    }
}

/// Spawning iterator. Produced by [`FlowWorld::spawn_batch`].
pub struct SpawnBatch<I> {
    bundles: I,
    world: FlowWorld,
}

impl<B, I> SpawnBatch<I>
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
    #[inline]
    pub fn spawn_all(&mut self) {
        self.for_each(|_| {});
    }
}

impl<B, I> Iterator for SpawnBatch<I>
where
    I: Iterator<Item = B>,
    B: Bundle,
{
    type Item = FlowEntity;

    #[inline]
    fn next(&mut self) -> Option<FlowEntity> {
        let bundle = self.bundles.next()?;

        // Safety: world reference does not escape this scope.
        let world = unsafe { self.world.get() };

        Some(FlowEntity::new(world.spawn_external(bundle).id()))
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<FlowEntity> {
        let bundle = self.bundles.nth(n)?;

        // Safety: world reference does not escape this scope.
        let world = unsafe { self.world.get() };

        Some(FlowEntity::new(world.spawn_external(bundle).id()))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bundles.size_hint()
    }

    #[inline]
    fn fold<T, F>(self, init: T, mut f: F) -> T
    where
        F: FnMut(T, FlowEntity) -> T,
    {
        self.bundles.fold(init, |acc, bundle| {
            // Safety: world reference does not escape this scope.
            // It is not fetched outside because iterator may fetch world as well.
            let world = unsafe { self.world.get() };

            f(acc, FlowEntity::new(world.spawn_external(bundle).id()))
        })
    }
}

impl<B, I> ExactSizeIterator for SpawnBatch<I>
where
    I: ExactSizeIterator<Item = B>,
    B: Bundle,
{
    #[inline]
    fn len(&self) -> usize {
        self.bundles.len()
    }
}

impl<B, I> DoubleEndedIterator for SpawnBatch<I>
where
    I: DoubleEndedIterator<Item = B>,
    B: Bundle,
{
    fn next_back(&mut self) -> Option<FlowEntity> {
        let bundle = self.bundles.next_back()?;

        // Safety: world reference does not escape this scope.
        let world = unsafe { self.world.get() };

        Some(FlowEntity::new(world.spawn_external(bundle).id()))
    }

    fn nth_back(&mut self, n: usize) -> Option<FlowEntity> {
        let bundle = self.bundles.nth_back(n)?;

        // Safety: world reference does not escape this scope.
        let world = unsafe { self.world.get() };

        Some(FlowEntity::new(world.spawn_external(bundle).id()))
    }

    fn rfold<T, F>(self, init: T, mut f: F) -> T
    where
        Self: Sized,
        F: FnMut(T, FlowEntity) -> T,
    {
        self.bundles.rfold(init, |acc, bundle| {
            // Safety: world reference does not escape this scope.
            // It is not fetched outside because iterator may fetch world as well.
            let world = unsafe { self.world.get() };

            f(acc, FlowEntity::new(world.spawn_external(bundle).id()))
        })
    }
}

impl<B, I> core::iter::FusedIterator for SpawnBatch<I>
where
    I: core::iter::FusedIterator<Item = B>,
    B: Bundle,
{
}
