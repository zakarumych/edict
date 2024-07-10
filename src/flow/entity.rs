use core::{
    any::TypeId,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    bundle::{Bundle, DynamicBundle, DynamicComponentBundle},
    component::Component,
    entity::{EntityBound, EntityId, EntityLoc, EntityRef},
    query::{DefaultQuery, IntoQuery, Query, QueryItem},
    world::WorldLocal,
    EntityError, NoSuchEntity,
};

use super::{get_flow_world, Flow, FlowWorld, WakeOnDrop};

/// Entity reference usable in flows.
///
/// It can be used to access entity's components,
/// insert and remove components.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FlowEntity {
    id: EntityId,
    marker: PhantomData<fn() -> &'static mut WorldLocal>,
}

impl crate::entity::Entity for FlowEntity {
    #[inline(always)]
    fn id(&self) -> EntityId {
        self.id
    }

    #[inline(always)]
    fn lookup(&self, entities: &crate::entity::EntitySet) -> Option<crate::entity::Location> {
        self.id.lookup(entities)
    }

    #[inline(always)]
    fn is_alive(&self, entities: &crate::entity::EntitySet) -> bool {
        self.id.is_alive(entities)
    }

    #[inline(always)]
    fn entity_loc<'a>(&self, entities: &'a crate::entity::EntitySet) -> Option<EntityLoc<'a>> {
        self.id.entity_loc(entities)
    }

    #[inline(always)]
    fn entity_ref<'a>(&self, world: &'a mut crate::world::World) -> Option<EntityRef<'a>> {
        self.id.entity_ref(world)
    }
}

impl PartialEq<EntityId> for FlowEntity {
    #[inline(always)]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == *other
    }
}

impl PartialEq<FlowEntity> for EntityId {
    #[inline(always)]
    fn eq(&self, other: &FlowEntity) -> bool {
        *self == other.id
    }
}

impl PartialEq<EntityBound<'_>> for FlowEntity {
    #[inline(always)]
    fn eq(&self, other: &EntityBound<'_>) -> bool {
        self.id == other.id()
    }
}

impl PartialEq<FlowEntity> for EntityBound<'_> {
    #[inline(always)]
    fn eq(&self, other: &FlowEntity) -> bool {
        self.id() == other.id
    }
}

impl PartialEq<EntityLoc<'_>> for FlowEntity {
    #[inline(always)]
    fn eq(&self, other: &EntityLoc<'_>) -> bool {
        self.id == other.id()
    }
}

impl PartialEq<FlowEntity> for EntityLoc<'_> {
    #[inline(always)]
    fn eq(&self, other: &FlowEntity) -> bool {
        self.id() == other.id
    }
}

impl PartialEq<EntityRef<'_>> for FlowEntity {
    #[inline(always)]
    fn eq(&self, other: &EntityRef<'_>) -> bool {
        self.id == other.id()
    }
}

impl PartialEq<FlowEntity> for EntityRef<'_> {
    #[inline(always)]
    fn eq(&self, other: &FlowEntity) -> bool {
        self.id() == other.id
    }
}

/// Future wrapped to be used as a flow.
#[doc(hidden)]
pub struct FutureEntityFlow<F> {
    id: EntityId,
    fut: F,
}

impl<F> Flow for FutureEntityFlow<F>
where
    F: Future<Output = ()> + Send,
{
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // Safety: `me` never moves.
        let me = unsafe { self.get_unchecked_mut() };

        {
            // Safety: world reference does not escape this scope.
            let world = unsafe { get_flow_world() };

            if !world.is_alive(me.id) {
                // Terminate flow if entity is removed.
                return Poll::Ready(());
            };
        }

        // Safety: Pin projection.
        let fut = unsafe { Pin::new_unchecked(&mut me.fut) };

        let poll = fut.poll(cx);

        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        let mut e = match world.entity(me.id) {
            Err(NoSuchEntity) => {
                // Terminate flow if entity is removed.
                return Poll::Ready(());
            }
            Ok(e) => e,
        };

        match poll {
            Poll::Pending => {
                // When entity is despawned, this task needs to be woken up
                // to cancel the flow.

                let auto_wake = e.with(WakeOnDrop::new);
                auto_wake.add_waker(cx.waker());
            }
            Poll::Ready(()) => {
                // If waker is registered, remove it for clean up.
                if let Some(auto_wake) = e.get_mut::<&mut WakeOnDrop>() {
                    auto_wake.remove_waker(cx.waker());
                }
            }
        }
        poll
    }
}

/// Type that can be spawned as a flow for an entity.
/// It can be an async function or a closure
/// that accepts [`FlowEntity`] as the only argument.
///
/// # Example
///
/// ```
/// # use edict::{world::World, flow::FlowEntity};
///
/// let mut world = edict::world::World::new();
///
/// let e = world.spawn(()).id();
///
/// world.spawn_flow_for(e, |e: FlowEntity| async move {
///   e.despawn();
/// });
/// ```
#[diagnostic::on_unimplemented(
    note = "Try `async fn(e: flow::Entity)` or `flow_fn!(|e: flow::Entity| {{ ... }})`"
)]
pub trait IntoEntityFlow: 'static {
    /// Flow type that will be polled.
    type Flow: Flow;

    /// Converts self into a flow.
    fn into_entity_flow(self, e: FlowEntity) -> Option<Self::Flow>;
}

impl<F, Fut> IntoEntityFlow for F
where
    F: FnOnce(FlowEntity) -> Fut + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Flow = FutureEntityFlow<Fut>;

    fn into_entity_flow(self, e: FlowEntity) -> Option<Self::Flow> {
        Some(FutureEntityFlow {
            id: e.id(),
            fut: self(e),
        })
    }
}

impl FlowEntity {
    #[doc(hidden)]
    #[inline(always)]
    pub fn new(id: EntityId) -> Self {
        FlowEntity {
            id,
            marker: PhantomData,
        }
    }

    /// Returns the entity id.
    #[inline(always)]
    pub fn id(self) -> EntityId {
        self.id
    }

    /// Returns the world reference.
    pub fn world(self) -> FlowWorld {
        FlowWorld::new()
    }

    /// Access entity reference in the world with closure.
    /// Returns closure result.
    ///
    /// # Panics
    ///
    /// If entity is not alive the closure will not be called and the method will panic.
    /// Use [`FlowEntity::try_map`] to handle entity not alive case.
    #[inline(always)]
    pub fn map<F, R>(self, f: F) -> R
    where
        F: FnOnce(EntityRef) -> R,
    {
        match self.try_map(f) {
            Ok(r) => r,
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Returns a future that will poll the closure with entity reference.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    ///
    /// If entity is not alive the future will not poll closure and never resolve.
    #[inline(always)]
    pub fn try_map<F, R>(self, f: F) -> Result<R, NoSuchEntity>
    where
        F: FnOnce(EntityRef) -> R,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        let e = world.entity(self.id)?;
        Ok(f(e))
    }

    /// Returns a future that will poll the closure with entity reference.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    ///
    /// If entity is not alive the future will not poll closure and never resolve.
    /// Use [`FlowEntity::try_poll`] to handle entity not alive case.
    #[inline(always)]
    pub fn poll<F, R>(self, f: F) -> PollEntityRef<F>
    where
        F: FnMut(EntityRef, &mut Context) -> Poll<R>,
    {
        PollEntityRef {
            entity: self.id,
            f,
            world: self.world(),
        }
    }

    /// Returns a future that will poll the closure with entity reference.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The future will resolve to `Err` if entity is despawned.
    ///
    /// The closure may use task context to register wakers.
    #[inline(always)]
    pub fn try_poll<F, R>(self, f: F) -> TryPollEntityRef<F>
    where
        F: FnMut(EntityRef, &mut Context) -> Poll<R>,
    {
        TryPollEntityRef {
            entity: self.id,
            f,
            world: self.world(),
        }
    }

    /// Returns a future that will poll the closure with entity view
    /// constructed with specified query type.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    ///
    /// If entity is not alive the future will not poll closure and never resolve.
    /// Use [`FlowEntity::try_poll_view`] to handle entity not alive case.
    ///
    /// Future will not poll closure and resolve until query is satisfied.
    pub fn poll_view<Q, F, R>(self, f: F) -> PollEntityView<Q::Query, F>
    where
        Q: DefaultQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        PollEntityView {
            entity: self.id,
            f,
            query: Q::default_query(),
            world: self.world(),
        }
    }

    /// Returns a future that will poll the closure with entity view
    /// constructed with specified query type.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The future will resolve to `Err` if entity is not alive or query is not satisfied.
    /// The closure may use task context to register wakers.
    pub fn try_poll_view<Q, F, R>(self, f: F) -> TryPollEntityView<Q::Query, F>
    where
        Q: DefaultQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        TryPollEntityView {
            entity: self.id,
            f,
            query: Q::default_query(),
            world: self.world(),
        }
    }

    /// Returns a future that will poll the closure with entity view
    /// constructed with specified query.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The closure may use task context to register wakers.
    ///
    /// If entity is not alive the future will not poll closure and never resolve.
    /// Use [`FlowEntity::try_poll_view_with`] to handle entity not alive case.
    ///
    /// Future will not poll closure and resolve until query is satisfied.
    pub fn poll_view_with<Q, F, R>(self, query: Q, f: F) -> PollEntityView<Q::Query, F>
    where
        Q: IntoQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        PollEntityView {
            entity: self.id,
            f,
            query: query.into_query(),
            world: self.world(),
        }
    }

    /// Returns a future that will poll the closure with entity view
    /// constructed with specified query.
    /// The future will resolve to closure result in [`Poll::Ready`].
    /// The future will resolve to `Err` if entity is not alive or query is not satisfied.
    /// The closure may use task context to register wakers.
    pub fn try_poll_view_with<Q, F, R>(self, query: Q, f: F) -> TryPollEntityView<Q::Query, F>
    where
        Q: IntoQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        TryPollEntityView {
            entity: self.id,
            f,
            query: query.into_query(),
            world: self.world(),
        }
    }

    /// Checks if entity is alive.
    pub fn is_alive(self) -> bool {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.is_alive(self.id)
    }

    /// Returns clone of the entity's component if it exists.
    /// Otherwise returns `None`.
    ///
    /// # Panics
    ///
    /// Panics if entity is not alive.
    #[inline(always)]
    pub fn get_cloned<T>(self) -> Option<T>
    where
        T: Clone + 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        match world.try_get_cloned(self.id) {
            Ok(c) => Some(c),
            Err(EntityError::Mismatch) => None,
            Err(EntityError::NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Returns clone of the entity's component if it exists.
    /// Otherwise returns error.
    /// If entity is not alive, returns `Err(NoSuchEntity)`.
    /// If entity does not have component of specified type, returns `Err(Mismatch)`.
    #[inline(always)]
    pub fn try_get_cloned<T>(self) -> Result<T, EntityError>
    where
        T: Clone + 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.try_get_cloned(self.id)
    }

    /// Sets new value for the entity component.
    ///
    /// Returns error if entity does not have component of specified type.
    #[inline(always)]
    pub fn set<T>(self, value: T) -> Result<(), T>
    where
        T: 'static,
    {
        match self.try_set(value) {
            Ok(_) => Ok(()),
            Err((EntityError::Mismatch, value)) => Err(value),
            Err((EntityError::NoSuchEntity, _)) => entity_not_alive(),
        }
    }

    /// Sets new value for the entity component.
    ///
    /// Returns error if entity does not have component of specified type.
    #[inline(always)]
    pub fn try_set<T>(self, value: T) -> Result<(), (EntityError, T)>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        match world.get::<&mut T>(self.id) {
            Ok(c) => {
                let c: &mut T = c;
                *c = value;
                Ok(())
            }
            Err(e) => Err((e, value)),
        }
    }

    /// Insert a component to the entity.
    #[inline(always)]
    pub fn insert<T>(self, component: T)
    where
        T: Component,
    {
        match self.try_insert(component) {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Insert a component to the entity.
    #[inline(always)]
    pub fn try_insert<T>(self, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.insert(self.id, component)
    }

    /// Attempts to inserts component to the entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    #[inline(always)]
    pub fn insert_external<T>(self, component: T)
    where
        T: 'static,
    {
        match self.try_insert_external(component) {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Attempts to inserts component to the entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    #[inline(always)]
    pub fn try_insert_external<T>(self, component: T) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.insert_external(self.id, component)
    }

    /// Inserts a component to the entity if it does not have one.
    #[inline(always)]
    pub fn with<T>(self, component: impl FnOnce() -> T)
    where
        T: Component,
    {
        match self.try_with(component) {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Inserts a component to the entity if it does not have one.
    #[inline(always)]
    pub fn try_with<T>(self, component: impl FnOnce() -> T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.with(self.id, component)?;
        Ok(())
    }

    /// Attempts to insert a component to the entity if it does not have one.
    #[inline(always)]
    pub fn with_external<T>(self, component: impl FnOnce() -> T)
    where
        T: 'static,
    {
        match self.try_with_external(component) {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Attempts to insert a component to the entity if it does not have one.
    #[inline(always)]
    pub fn try_with_external<T>(self, component: impl FnOnce() -> T) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.with_external(self.id, component)?;
        Ok(())
    }

    /// Inserts bundle of components to the entity.
    /// This is moral equivalent to calling `WorldLocal::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn insert_bundle<B>(self, bundle: B)
    where
        B: DynamicComponentBundle,
    {
        match self.try_insert_bundle(bundle) {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Inserts bundle of components to the entity.
    /// This is moral equivalent to calling `WorldLocal::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn try_insert_bundle<B>(self, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.insert_bundle(self.id, bundle)
    }

    /// Inserts bundle of components to the entity.
    /// This is moral equivalent to calling `WorldLocal::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn insert_external_bundle<B>(self, bundle: B)
    where
        B: DynamicBundle,
    {
        match self.try_insert_external_bundle(bundle) {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Inserts bundle of components to the entity.
    /// This is moral equivalent to calling `WorldLocal::insert` with each component separately,
    /// but more efficient.
    ///
    /// For each component type in bundle:
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn try_insert_external_bundle<B>(self, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.insert_external_bundle(self.id, bundle)
    }

    /// Removes a component from the entity.
    /// Returns the component if it was present.
    #[inline(always)]
    pub fn remove<T>(self) -> Option<T>
    where
        T: 'static,
    {
        match self.try_remove::<T>() {
            Ok(c) => Some(c),
            Err(EntityError::Mismatch) => None,
            Err(EntityError::NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Removes a component from the entity.
    /// Returns the component if it was present.
    #[inline(always)]
    pub fn try_remove<T>(self) -> Result<T, EntityError>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        let (c, _) = world.remove::<T>(self.id)?;
        match c {
            None => Err(EntityError::Mismatch),
            Some(c) => Ok(c),
        }
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop<T>(self)
    where
        T: 'static,
    {
        match self.try_drop::<T>() {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn try_drop<T>(self) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.drop::<T>(self.id)
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop_erased(self, ty: TypeId) {
        match self.try_drop_erased(ty) {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn try_drop_erased(self, ty: TypeId) -> Result<(), NoSuchEntity> {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.drop_erased(self.id, ty)
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
    #[inline(always)]
    pub fn drop_bundle<B>(self)
    where
        B: Bundle,
    {
        match self.try_drop_bundle::<B>() {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
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
    #[inline(always)]
    pub fn try_drop_bundle<B>(self) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.drop_bundle::<B>(self.id)
    }

    /// Despawns the referenced entity.
    #[inline(always)]
    pub fn despawn(self) {
        match self.try_despawn() {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Despawns the referenced entity.
    #[inline(always)]
    pub fn try_despawn(self) -> Result<(), NoSuchEntity> {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.despawn(self.id)
    }

    /// Checks if entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn has_component<T: 'static>(self) -> bool {
        match self.try_has_component::<T>() {
            Ok(b) => b,
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Checks if entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn try_has_component<T: 'static>(self) -> Result<bool, NoSuchEntity> {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.try_has_component::<T>(self.id)
    }

    /// Spawns a new flow for the entity.
    pub fn spawn_flow<F>(self, f: F)
    where
        F: IntoEntityFlow,
    {
        // Safety: world reference does not escape this scope.
        let world = unsafe { get_flow_world() };

        world.spawn_flow_for(self.id, f);
    }
}

/// Future that polls the closure with entity reference.
/// Resolves to closure result in [`Poll::Ready`].
/// The closure may use task context to register wakers.
///
/// If entity is not alive the future will not poll closure and never resolve.
#[must_use = "Future does nothing unless polled"]
pub struct PollEntityRef<F> {
    entity: EntityId,
    f: F,
    world: FlowWorld,
}

impl<F, R> Future for PollEntityRef<F>
where
    F: FnMut(EntityRef, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        // Safety: `me` never moves.
        let me = unsafe { self.get_unchecked_mut() };

        // Safety: world reference does not escape this scope.
        let world = unsafe { me.world.get() };

        let Ok(e) = world.entity(me.entity) else {
            return Poll::Pending;
        };
        (me.f)(e, cx)
    }
}

/// Future that will poll the closure with entity reference.
/// Resolves to closure result in [`Poll::Ready`].
/// Resolves to `Err` if entity is despawned.
///
/// The closure may use task context to register wakers.
#[must_use = "Future does nothing unless polled"]
pub struct TryPollEntityRef<F> {
    entity: EntityId,
    f: F,
    world: FlowWorld,
}

impl<F, R> Future for TryPollEntityRef<F>
where
    F: FnMut(EntityRef, &mut Context) -> Poll<R>,
{
    type Output = Result<R, NoSuchEntity>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<R, NoSuchEntity>> {
        // Safety: `me` never moves.
        let me = unsafe { self.get_unchecked_mut() };

        // Safety: world reference does not escape this scope.
        let world = unsafe { me.world.get() };

        let e = world.entity(me.entity)?;
        let poll = (me.f)(e, cx);
        try_poll(poll, me.entity, world, cx)
    }
}

/// Future that will poll the closure with entity view
/// constructed with specified query.
/// Resolves to closure result in [`Poll::Ready`].
/// The closure may use task context to register wakers.
///
/// If entity is not alive the future will not poll closure and never resolve.
/// Future will not poll closure and resolve until query is satisfied.
#[must_use = "Future does nothing unless polled"]
pub struct PollEntityView<Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    world: FlowWorld,
}

impl<Q, F, R> Future for PollEntityView<Q, F>
where
    Q: Query,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        // Safety: `me` never moves.
        let me = unsafe { self.get_unchecked_mut() };

        // Safety: world reference does not escape this scope.
        let world = unsafe { me.world.get() };

        match world.get_with(me.entity, me.query) {
            Err(EntityError::NoSuchEntity) => Poll::Pending,
            Err(EntityError::Mismatch) => {
                // TODO: Should archetype change be detected to wake up the task?
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Ok(item) => (me.f)(item, cx),
        }
    }
}

/// Future that will poll the closure with entity view
/// constructed with specified query.
/// Resolves to closure result in [`Poll::Ready`].
/// Resolves to `Err` if entity is not alive or query is not satisfied.
/// The closure may use task context to register wakers.
#[must_use = "Future does nothing unless polled"]
pub struct TryPollEntityView<Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    world: FlowWorld,
}

impl<Q, F, R> Future for TryPollEntityView<Q, F>
where
    Q: Query,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = Result<R, EntityError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<R, EntityError>> {
        // Safety: `me` never moves.
        let me = unsafe { self.get_unchecked_mut() };

        // Safety: world reference does not escape this scope.
        let world = unsafe { me.world.get() };

        let item = world.get_with(me.entity, me.query)?;
        let poll = (me.f)(item, cx);
        let poll = try_poll(poll, me.entity, world, cx)?;
        poll.map(Ok)
    }
}

fn try_poll<R>(
    poll: Poll<R>,
    entity: EntityId,
    world: &mut WorldLocal,
    cx: &mut Context<'_>,
) -> Poll<Result<R, NoSuchEntity>> {
    let mut e = world.entity(entity)?;

    match poll {
        Poll::Pending => {
            // When entity is despawned, this task needs to be woken up
            // to resolve the future.

            let auto_wake = e.with(WakeOnDrop::new);
            auto_wake.add_waker(cx.waker());

            Poll::Pending
        }
        Poll::Ready(result) => {
            // If waker is registered, remove it for clean up.
            if let Some(auto_wake) = e.get_mut::<&mut WakeOnDrop>() {
                auto_wake.remove_waker(cx.waker());
            }

            Poll::Ready(Ok(result))
        }
    }
}

/// Handles despawned entity in methods that assume entity is alive.
#[cold]
#[inline(never)]
fn entity_not_alive() -> ! {
    panic!("Entity is not alive");
}
