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
    entity::{EntityBound, EntityId, EntityRef},
    query::{DefaultQuery, ImmutableQuery, Query, QueryItem},
    world::WorldLocal,
    EntityError, NoSuchEntity,
};

use super::{flow_entity, tls::EntityGuard, Flow, FlowClosure, FlowContext, WakeOnDrop, World};

/// Entity reference usable in flows.
///
/// It can be used to access entity's components,
/// insert and remove components.
#[derive(Debug, PartialEq, Eq)]
pub struct Entity<'a> {
    id: EntityId,
    marker: PhantomData<&'a mut WorldLocal>,
}

unsafe impl Send for Entity<'_> {}
unsafe impl Sync for Entity<'_> {}

impl PartialEq<EntityId> for Entity<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == *other
    }
}

impl PartialEq<EntityBound<'_>> for Entity<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityBound<'_>) -> bool {
        self.id == other.id()
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
        let this = unsafe { self.get_unchecked_mut() };

        if !unsafe { super::flow_world_ref() }.is_alive(this.id) {
            // Terminate flow if entity is removed.
            return Poll::Ready(());
        };

        let poll = {
            let _guard = EntityGuard::new(this.id);

            // Safety: pin projection
            let fut = unsafe { Pin::new_unchecked(&mut this.fut) };
            fut.poll(cx)
        };

        let world = unsafe { super::flow_world_mut() };

        let mut e = match world.entity(this.id) {
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
/// It can be an async function or a closure inside `flow_fn!` macro.
/// It must accept [`Entity`] as the only argument.
///
/// # Example
///
/// ```
/// # use edict::{world::World, flow::{Entity, flow}};
///
/// let mut world = edict::world::World::new();
///
/// let e = world.spawn(()).id();
///
/// world.spawn_flow_for(e, flow_fn!(|e: Entity| {
///   e.despawn();
/// })
/// ```
#[diagnostic::on_unimplemented(
    note = "Try `async fn(e: flow::Entity)` or `flow_fn!(|e: flow::Entity| {{ ... }})`"
)]
pub trait IntoEntityFlow: 'static {
    /// Flow type that will be polled.
    type Flow<'a>: Flow + 'a;

    /// Converts self into a flow.
    fn into_entity_flow<'a>(self, e: Entity<'a>) -> Option<Self::Flow<'a>>;
}

/// Trait implemented by functions that can be used to spawn flows.
/// Argument represents the entity that can be used to fetch its components.
/// The world can be accessed through the entity.
pub trait EntityFlowFn<'a> {
    /// Future type returned from the function.
    type Fut: Future<Output = ()> + Send + 'a;

    /// Runs the function with given entity.
    fn run(self, entity: Entity<'a>) -> Self::Fut;
}

// Must be callable with any lifetime of `Entity` borrow.
impl<'a, F, Fut> EntityFlowFn<'a> for F
where
    F: FnOnce(Entity<'a>) -> Fut,
    Fut: Future<Output = ()> + Send + 'a,
{
    type Fut = Fut;

    fn run(self, entity: Entity<'a>) -> Fut {
        self(entity)
    }
}

impl<F> IntoEntityFlow for F
where
    for<'a> F: EntityFlowFn<'a> + 'static,
{
    type Flow<'a> = FutureEntityFlow<<F as EntityFlowFn<'a>>::Fut>;

    fn into_entity_flow<'a>(self, e: Entity<'a>) -> Option<Self::Flow<'a>> {
        Some(FutureEntityFlow {
            id: e.id(),
            fut: self.run(e),
        })
    }
}

#[doc(hidden)]
pub struct FlowEntity(EntityId);

impl<'a> FlowContext<'a> for Entity<'a> {
    type Token = FlowEntity;

    fn cx(token: &'a FlowEntity) -> Self {
        Entity {
            id: token.0,
            marker: PhantomData,
        }
    }
}

impl<F, Fut> IntoEntityFlow for FlowClosure<F, Fut>
where
    F: FnOnce(FlowEntity) -> Fut + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Flow<'a> = FutureEntityFlow<Fut>;

    fn into_entity_flow(self, e: Entity<'_>) -> Option<FutureEntityFlow<Fut>> {
        Some(FutureEntityFlow {
            id: e.id(),
            fut: (self.f)(FlowEntity(e.id())),
        })
    }
}

impl Entity<'_> {
    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn make(id: EntityId) -> Self {
        Entity {
            id,
            marker: PhantomData,
        }
    }

    /// Creates a new [`Entity`] value that borrows from this.
    #[inline(always)]
    pub fn reborrow(&mut self) -> Entity<'_> {
        Entity {
            id: self.id,
            marker: PhantomData,
        }
    }

    /// Returns the entity id.
    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Returns reference to the world.
    #[inline(always)]
    pub fn get_world(&self) -> &World {
        unsafe { World::make_ref() }
    }

    /// Returns mutable reference to the world.
    #[inline(always)]
    pub fn get_world_mut(&mut self) -> &mut World {
        unsafe { World::make_mut() }
    }

    /// Polls with entity ref until closure returns [`Poll::Ready`].
    /// Will never resolve if entity is despawned.
    #[inline(always)]
    pub fn poll_ref<F, R>(&mut self, f: F) -> PollRef<'_, F>
    where
        F: FnMut(EntityRef, &mut Context) -> Poll<R>,
    {
        PollRef {
            entity: self.id,
            f,
            marker: PhantomData,
        }
    }

    /// Polls with entity ref until closure returns [`Poll::Ready`].
    /// Resolves to `None` if entity is despawned.
    #[inline(always)]
    pub fn try_poll_ref<F, R>(&mut self, f: F) -> TryPollRef<'_, F>
    where
        F: FnMut(EntityRef, &mut Context) -> Poll<R>,
    {
        TryPollRef {
            entity: self.id,
            f,
            marker: PhantomData,
        }
    }

    /// Polls with view until closure returns [`Poll::Ready`].
    /// Waits until query is satisfied.
    /// Will never resolve if entity is despawned.
    pub fn poll_view<Q, F, R>(&self, f: F) -> PollView<'_, Q::Query, F>
    where
        Q: DefaultQuery,
        Q::Query: ImmutableQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        PollView {
            entity: self.id,
            f,
            query: Q::default_query(),
            marker: PhantomData,
        }
    }

    /// Polls with view until closure returns [`Poll::Ready`].
    /// Waits until query is satisfied.
    /// Will never resolve if entity is despawned.
    pub fn poll_view_mut<Q, F, R>(&mut self, f: F) -> PollViewMut<'_, Q::Query, F>
    where
        Q: DefaultQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        PollViewMut {
            entity: self.id,
            f,
            query: Q::default_query(),
            marker: PhantomData,
        }
    }

    /// Polls with view until closure returns [`Poll::Ready`].
    /// Resolves to `None` if query is not satisfied or entity is despawned .
    pub fn try_poll_view<Q, F, R>(&self, f: F) -> TryPollView<'_, Q::Query, F>
    where
        Q: DefaultQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        TryPollView {
            entity: self.id,
            f,
            query: Q::default_query(),
            marker: PhantomData,
        }
    }

    /// Polls with view until closure returns [`Poll::Ready`].
    /// Resolves to `None` if query is not satisfied or entity is despawned .
    pub fn try_poll_view_mut<Q, F, R>(&mut self, f: F) -> TryPollViewMut<'_, Q::Query, F>
    where
        Q: DefaultQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        TryPollViewMut {
            entity: self.id,
            f,
            query: Q::default_query(),
            marker: PhantomData,
        }
    }

    /// Returns normal entity reference.
    ///
    /// # Panics
    ///
    /// Panics if entity is not alive.
    #[inline(always)]
    pub fn get_ref(&mut self) -> EntityRef<'_> {
        match self.try_get_ref() {
            Ok(e) => e,
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Returns normal entity reference.
    /// Returns error if entity is not alive.
    #[inline(always)]
    pub fn try_get_ref(&mut self) -> Result<EntityRef<'_>, NoSuchEntity> {
        let id = self.id;
        EntityRef::new(id, self.get_world_mut())
    }

    /// Queries component from the entity.
    ///
    /// Returns clone of the component value.
    #[inline(always)]
    pub fn get_cloned<T>(&self) -> Option<T>
    where
        T: Clone + 'static,
    {
        match self.try_get_cloned() {
            Ok(c) => Some(c),
            Err(EntityError::Mismatch) => None,
            Err(EntityError::NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Queries component from the entity.
    ///
    /// Returns clone of the component value.
    #[inline(always)]
    pub fn try_get_cloned<T>(&self) -> Result<T, EntityError>
    where
        T: Clone + 'static,
    {
        let mut view = self.get_world().try_view_one::<&T>(self.id)?;
        match view.get_mut() {
            Some(c) => Ok(c.clone()),
            None => Err(EntityError::Mismatch),
        }
    }

    /// Queries component from the entity.
    ///
    /// Returns copy of the component value.
    #[inline(always)]
    pub fn get_copied<T>(&self) -> Option<T>
    where
        T: Copy + Sync + 'static,
    {
        match self.try_get_copied() {
            Ok(c) => Some(c),
            Err(EntityError::Mismatch) => None,
            Err(EntityError::NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Queries component from the entity.
    ///
    /// Returns copy of the component value.
    #[inline(always)]
    pub fn try_get_copied<T>(&self) -> Result<T, EntityError>
    where
        T: Copy + 'static,
    {
        let mut view = self.get_world().try_view_one::<&T>(self.id)?;
        match view.get_mut() {
            Some(c) => Ok(*c),
            None => Err(EntityError::Mismatch),
        }
    }

    /// Sets new value for the entity component.
    ///
    /// Returns error if entity does not have component of specified type.
    #[inline(always)]
    pub fn set<T>(&mut self, value: T) -> Result<(), T>
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
    pub fn try_set<T>(&mut self, value: T) -> Result<(), (EntityError, T)>
    where
        T: 'static,
    {
        let id = self.id;
        match self.get_world_mut().get::<&mut T>(id) {
            Ok(c) => {
                *c = value;
                Ok(())
            }
            Err(e) => Err((e, value)),
        }
    }

    /// Insert a component to the entity.
    #[inline(always)]
    pub fn insert<T>(&mut self, component: T)
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
    pub fn try_insert<T>(&mut self, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        let id = self.id;
        self.get_world_mut().insert(id, component)
    }

    /// Attempts to inserts component to the entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    #[inline(always)]
    pub fn insert_external<T>(&mut self, component: T)
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
    pub fn try_insert_external<T>(&mut self, component: T) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        let id = self.id;
        self.get_world_mut().insert_external(id, component)
    }

    /// Inserts a component to the entity if it does not have one.
    #[inline(always)]
    pub fn with<T>(&mut self, component: impl FnOnce() -> T)
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
    pub fn try_with<T>(&mut self, component: impl FnOnce() -> T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        let id = self.id;
        self.get_world_mut().with(id, component)?;
        Ok(())
    }

    /// Attempts to insert a component to the entity if it does not have one.
    #[inline(always)]
    pub fn with_external<T>(&mut self, component: impl FnOnce() -> T)
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
    pub fn try_with_external<T>(
        &mut self,
        component: impl FnOnce() -> T,
    ) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        let id = self.id;
        self.get_world_mut().with_external(id, component)?;
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
    pub fn insert_bundle<B>(&mut self, bundle: B)
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
    pub fn try_insert_bundle<B>(&mut self, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        let id = self.id;
        self.get_world_mut().insert_bundle(id, bundle)
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
    pub fn insert_external_bundle<B>(&mut self, bundle: B)
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
    pub fn try_insert_external_bundle<B>(&mut self, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        let id = self.id;
        self.get_world_mut().insert_external_bundle(id, bundle)
    }

    /// Removes a component from the entity.
    /// Returns the component if it was present.
    #[inline(always)]
    pub fn remove<T>(&mut self) -> Option<T>
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
    pub fn try_remove<T>(&mut self) -> Result<T, EntityError>
    where
        T: 'static,
    {
        let id = self.id;
        let (c, _) = self.get_world_mut().remove::<T>(id)?;
        match c {
            None => Err(EntityError::Mismatch),
            Some(c) => Ok(c),
        }
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop<T>(&mut self)
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
    pub fn try_drop<T>(&mut self) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        let id = self.id;
        self.get_world_mut().drop::<T>(id)
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop_erased(&mut self, ty: TypeId) {
        match self.try_drop_erased(ty) {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn try_drop_erased(&mut self, ty: TypeId) -> Result<(), NoSuchEntity> {
        let id = self.id;
        self.get_world_mut().drop_erased(id, ty)
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
    pub fn drop_bundle<B>(&mut self)
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
    pub fn try_drop_bundle<B>(&mut self) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        let id = self.id;
        self.get_world_mut().drop_bundle::<B>(id)
    }

    /// Despawns the referenced entity.
    #[inline(always)]
    pub fn despawn(&mut self) {
        match self.try_despawn() {
            Ok(_) => (),
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Despawns the referenced entity.
    #[inline(always)]
    pub fn try_despawn(&mut self) -> Result<(), NoSuchEntity> {
        let id = self.id;
        self.get_world_mut().despawn(id)
    }

    /// Checks if entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn has_component<T: 'static>(&self) -> bool {
        match self.try_has_component::<T>() {
            Ok(b) => b,
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    /// Checks if entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn try_has_component<T: 'static>(&self) -> Result<bool, NoSuchEntity> {
        let id = self.id;
        self.get_world().try_has_component::<T>(id)
    }

    /// Spawns a new flow for the entity.
    pub fn spawn_flow<F>(&mut self, f: F)
    where
        F: IntoEntityFlow,
    {
        let id = self.id;
        self.get_world_mut().spawn_flow_for(id, f);
    }
}

/// Flow future that provides [`EntityRef`] to the bound closure on each poll.
/// Resolves to the ready result of the closure.
/// Never resolves if entity is despawned.
#[must_use = "Future does nothing unless polled"]
pub struct PollRef<'a, F> {
    entity: EntityId,
    f: F,
    marker: PhantomData<fn() -> &'a mut WorldLocal>,
}

impl<F, R> Future for PollRef<'_, F>
where
    F: FnMut(EntityRef, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            let Ok(e) = super::flow_world_mut().entity(me.entity) else {
                return on_no_such_entity(me.entity, cx);
            };
            (me.f)(e, cx)
        }
    }
}

/// Flow future that provides [`EntityRef`] to the bound closure on each poll.
/// Resolves to the ready result of the closure wrapped in `Some`.
/// Resolves to `None` if entity is despawned.
#[must_use = "Future does nothing unless polled"]
pub struct TryPollRef<'a, F> {
    entity: EntityId,
    f: F,
    marker: PhantomData<fn() -> &'a mut WorldLocal>,
}

impl<F, R> Future for TryPollRef<'_, F>
where
    F: FnMut(EntityRef, &mut Context) -> Poll<R>,
{
    type Output = Option<R>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<R>> {
        unsafe {
            let me = self.get_unchecked_mut();
            let Ok(e) = super::flow_world_mut().entity(me.entity) else {
                return Poll::Ready(None);
            };
            (me.f)(e, cx).map(Some)
        }
    }
}

/// Flow future that provides view to the entity's components to the bound closure on each poll.
/// Limited to immutable queries.
/// Resolves to the ready result of the closure.
/// Yields until query is satisfied.
/// Never resolves if entity is despawned.
#[must_use = "Future does nothing unless polled"]
pub struct PollView<'a, Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<fn() -> &'a WorldLocal>,
}

impl<Q, F, R> Future for PollView<'_, Q, F>
where
    Q: Query,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            match super::flow_world_ref().try_view_one_with(me.entity, me.query) {
                Err(NoSuchEntity) => on_no_such_entity(me.entity, cx),
                Ok(mut view_one) => match view_one.get_mut() {
                    None => {
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                    Some(item) => (me.f)(item, cx),
                },
            }
        }
    }
}

/// Flow future that provides view to the entity's components to the bound closure on each poll.
/// Not limited to immutable queries.
/// Resolves to the ready result of the closure.
/// Yields until query is satisfied.
/// Never resolves if entity is despawned.
#[must_use = "Future does nothing unless polled"]
pub struct PollViewMut<'a, Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<fn() -> &'a mut WorldLocal>,
}

impl<Q, F, R> Future for PollViewMut<'_, Q, F>
where
    Q: Query,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            match super::flow_world_mut().get_with(me.entity, me.query) {
                Err(EntityError::NoSuchEntity) => on_no_such_entity(me.entity, cx),
                Err(EntityError::Mismatch) => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Ok(item) => (me.f)(item, cx),
            }
        }
    }
}

/// Flow future that provides view to the entity's components to the bound closure on each poll.
/// Limited to immutable queries.
/// Resolves to the ready result of the closure wrapped in `Some`.
/// Resolves to `None` if query is not satisfied or entity is despawned.
#[must_use = "Future does nothing unless polled"]
pub struct TryPollView<'a, Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<fn() -> &'a WorldLocal>,
}

impl<Q, F, R> Future for TryPollView<'_, Q, F>
where
    Q: Query,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = Option<R>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<R>> {
        unsafe {
            let me = self.get_unchecked_mut();
            match super::flow_world_ref().try_view_one_with(me.entity, me.query) {
                Err(NoSuchEntity) => return Poll::Ready(None),
                Ok(mut view_one) => match view_one.get_mut() {
                    None => Poll::Ready(None),
                    Some(item) => (me.f)(item, cx).map(Some),
                },
            }
        }
    }
}

/// Flow future that provides view to the entity's components to the bound closure on each poll.
/// Not limited to immutable queries.
/// Resolves to the ready result of the closure wrapped in `Some`.
/// Resolves to `None` if query is not satisfied or entity is despawned.
#[must_use = "Future does nothing unless polled"]
pub struct TryPollViewMut<'a, Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<fn() -> &'a mut WorldLocal>,
}

impl<Q, F, R> Future for TryPollViewMut<'_, Q, F>
where
    Q: Query,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = Option<R>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<R>> {
        unsafe {
            let me = self.get_unchecked_mut();
            match super::flow_world_mut().get_with(me.entity, me.query) {
                Err(EntityError::NoSuchEntity) => return Poll::Ready(None),
                Err(EntityError::Mismatch) => Poll::Ready(None),
                Ok(item) => (me.f)(item, cx).map(Some),
            }
        }
    }
}

/// Handles despawned entity in futures.
fn on_no_such_entity<T>(entity: EntityId, cx: &mut Context) -> Poll<T> {
    match flow_entity() {
        Some(id) if id == entity => {
            // Entity is removed.
            // Wake and pending.
            // Flow will resume next tick and cancel itself.
            cx.waker().wake_by_ref();
        }
        _ => {
            // Entity is removed.
            // Flow will never resume.
        }
    }
    Poll::Pending
}

/// Handles despawned entity in methods that assume entity is alive.
#[cold]
#[inline(never)]
fn entity_not_alive() -> ! {
    panic!("Entity is not alive");
}
