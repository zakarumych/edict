use core::{
    any::TypeId,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use smallvec::SmallVec;

use crate::{
    action::LocalActionEncoder,
    bundle::{Bundle, DynamicBundle, DynamicComponentBundle},
    component::Component,
    entity::{EntityBound, EntityId, EntityRef},
    query::{DefaultQuery, ImmutableQuery, QueryItem, SendQuery},
    world::World,
    EntityError, NoSuchEntity,
};

use super::{Flow, FlowWorld};

/// Entity reference usable in flows.
///
/// It can be used to access entity's components,
/// insert and remove components.
#[derive(Debug, PartialEq, Eq)]
pub struct FlowEntity<'a> {
    id: EntityId,
    marker: PhantomData<&'a mut World>,
}

unsafe impl Send for FlowEntity<'_> {}
unsafe impl Sync for FlowEntity<'_> {}

impl PartialEq<EntityId> for FlowEntity<'_> {
    #[inline(always)]
    fn eq(&self, other: &EntityId) -> bool {
        self.id == *other
    }
}

impl PartialEq<EntityBound<'_>> for FlowEntity<'_> {
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
    F: Future<Output = ()> + Send + 'static,
{
    unsafe fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = unsafe { self.get_unchecked_mut() };

        if !super::flow_world_ref().is_alive(this.id) {
            // Terminate flow if entity is removed.
            return Poll::Ready(());
        };

        let fut = unsafe { Pin::new_unchecked(&mut this.fut) };
        let poll = fut.poll(cx);

        let world = super::flow_world_mut();

        let mut e = match world.entity(this.id) {
            Err(NoSuchEntity) => {
                // Terminate flow if entity is removed.
                return Poll::Ready(());
            }
            Ok(e) => e,
        };

        match poll {
            Poll::Pending => {
                // Ensure to wake on entity drop.
                let auto_wake = e.with(AutoWake::new);
                auto_wake.add_waker(cx.waker());
            }
            Poll::Ready(()) => {
                // Remove auto-waker for this future.
                if let Some(auto_wake) = e.get_mut::<&mut AutoWake>() {
                    auto_wake.remove_waker(cx.waker());
                }
            }
        }
        poll
    }
}

pub trait IntoEntityFlow: 'static {
    type Flow: Flow;

    unsafe fn into_entity_flow(self, id: EntityId) -> Self::Flow;
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
pub trait EntityFlowFn<'a> {
    type Fut: Future<Output = ()> + Send + 'a;
    fn run(self, entity: FlowEntity<'a>) -> Self::Fut;
}

// Must be callable with any lifetime of `FlowEntity` borrow.
impl<'a, F, Fut> EntityFlowFn<'a> for F
where
    F: FnOnce(FlowEntity<'a>) -> Fut,
    Fut: Future<Output = ()> + Send + 'a,
{
    type Fut = Fut;

    fn run(self, entity: FlowEntity<'a>) -> Fut {
        self(entity)
    }
}

impl<F> IntoEntityFlow for F
where
    for<'a> F: EntityFlowFn<'a> + 'static,
{
    type Flow = FutureEntityFlow<<F as EntityFlowFn<'static>>::Fut>;

    unsafe fn into_entity_flow(self, id: EntityId) -> Self::Flow {
        FutureEntityFlow {
            id,
            fut: self.run(FlowEntity::make(id)),
        }
    }
}

struct BadEntityFlow<F, Fut> {
    f: F,
    _phantom: PhantomData<Fut>,
}

impl<F, Fut> IntoEntityFlow for BadEntityFlow<F, Fut>
where
    F: FnOnce(FlowEntity<'static>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Flow = FutureEntityFlow<Fut>;

    unsafe fn into_entity_flow(self, id: EntityId) -> Self::Flow {
        FutureEntityFlow {
            id,
            fut: (self.f)(FlowEntity::make(id)),
        }
    }
}

#[doc(hidden)]
pub unsafe fn bad_entity_flow_closure<F, Fut>(f: F) -> impl IntoEntityFlow
where
    F: FnOnce(FlowEntity<'static>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    BadEntityFlow {
        f,
        _phantom: PhantomData,
    }
}

/// Converts closure syntax to flow fn.
///
/// There's limitation that makes following `|world: FlowEntity<'_>| async move { /*use world*/ }`
/// to be noncompilable.
///
/// On nightly it is possible to use `async move |world: FlowEntity<'_>| { /*use world*/ }`
/// But this syntax is not stable yet and edict avoids requiring too many nighty features.
///
/// This macro is a workaround for this limitation.
#[macro_export]
macro_rules! flow_closure_for {
    (|mut $entity:ident $(: $FlowEntity:ty)?| -> $ret:ty $code:block) => {
        unsafe {
            $crate::flow::bad_entity_flow_closure(move |mut entity: $crate::flow::FlowEntity<'static>| async move {
                #[allow(unused_mut)]
                let mut $entity $(: $FlowEntity)? = entity.reborrow();
                let res: $ret = { $code };
                res
            })
        }
    };
    (|mut $entity:ident $(: $FlowEntity:ty)?| $code:expr) => {
        unsafe {
            $crate::flow::bad_entity_flow_closure(move |mut entity: $crate::flow::FlowEntity<'static>| async move {
                #[allow(unused_mut)]
                let mut $entity $(: $FlowEntity)? = entity.reborrow();
                $code
            })
        }
    };
}

/// This component is used to wake all entity flows when entity is removed.
struct AutoWake {
    wakers: SmallVec<[Waker; 4]>,
}

impl AutoWake {
    fn new() -> Self {
        AutoWake {
            wakers: SmallVec::new(),
        }
    }

    fn add_waker(&mut self, other: &Waker) {
        for waker in &mut self.wakers {
            if waker.will_wake(other) {
                return;
            }
        }
        self.wakers.push(other.clone());
    }

    fn remove_waker(&mut self, other: &Waker) {
        if let Some(idx) = self.wakers.iter().position(|waker| waker.will_wake(other)) {
            self.wakers.swap_remove(idx);
        }
    }
}

impl Component for AutoWake {
    #[inline(always)]
    fn name() -> &'static str {
        "AutoWake"
    }

    #[inline(always)]
    fn on_drop(&mut self, _id: EntityId, _encoder: LocalActionEncoder) {
        // Wake all flows bound to this entity to
        // allow them to terminate.
        for waker in self.wakers.drain(..) {
            waker.wake();
        }
    }
}

impl FlowEntity<'_> {
    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn make(id: EntityId) -> Self {
        FlowEntity {
            id,
            marker: PhantomData,
        }
    }

    #[doc(hidden)]
    #[inline(always)]
    pub fn reborrow(&mut self) -> FlowEntity<'_> {
        FlowEntity {
            id: self.id,
            marker: PhantomData,
        }
    }

    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
    }

    #[inline(always)]
    pub fn get_world(&self) -> &FlowWorld {
        unsafe { FlowWorld::make_ref() }
    }

    #[inline(always)]
    pub fn get_world_mut(&mut self) -> &mut FlowWorld {
        unsafe { FlowWorld::make_mut() }
    }

    /// Polls with entity ref until closure returns [`Poll::Ready`].
    #[inline(always)]
    pub fn poll_ref<F, Q, R>(&mut self, f: F) -> PollRef<'_, F>
    where
        F: FnMut(EntityRef, &mut Context) -> Poll<R>,
    {
        PollRef {
            entity: self.id,
            f,
            marker: PhantomData,
        }
    }

    /// Polls with view until closure returns [`Poll::Ready`].
    /// Waits until query is satisfied.
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
    /// If query is not satisfied, resolves to `None`.
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
    /// If query is not satisfied, resolves to `None`.
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

    #[inline(always)]
    pub fn get_ref(&mut self) -> EntityRef<'_> {
        match self.try_get_ref() {
            Ok(e) => e,
            Err(NoSuchEntity) => entity_not_alive(),
        }
    }

    #[inline(always)]
    pub fn try_get_ref(&mut self) -> Result<EntityRef<'_>, NoSuchEntity> {
        let id = self.id;
        self.get_world_mut().entity(id)
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
    /// This is moral equivalent to calling `World::insert` with each component separately,
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
    /// This is moral equivalent to calling `World::insert` with each component separately,
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
    /// This is moral equivalent to calling `World::insert` with each component separately,
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
    /// This is moral equivalent to calling `World::insert` with each component separately,
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
    pub fn spawn_flow<F>(&self, f: F)
    where
        F: for<'a> EntityFlowFn<'a> + 'static,
    {
        super::spawn_local_for(self.get_world(), self.id, f);
    }
}

#[must_use = "Future does nothing unless polled"]
pub struct PollRef<'a, F> {
    entity: EntityId,
    f: F,
    marker: PhantomData<fn() -> &'a mut World>,
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
                return on_no_such_entity(cx);
            };
            (me.f)(e, cx)
        }
    }
}

#[must_use = "Future does nothing unless polled"]
pub struct PollView<'a, Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<fn() -> &'a World>,
}

impl<Q, F, R> Future for PollView<'_, Q, F>
where
    Q: SendQuery,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            match super::flow_world_ref().try_view_one_with(me.entity, me.query) {
                Err(NoSuchEntity) => on_no_such_entity(cx),
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

#[must_use = "Future does nothing unless polled"]
pub struct PollViewMut<'a, Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<fn() -> &'a mut World>,
}

impl<Q, F, R> Future for PollViewMut<'_, Q, F>
where
    Q: SendQuery,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            match super::flow_world_mut().get_with(me.entity, me.query) {
                Err(EntityError::NoSuchEntity) => on_no_such_entity(cx),
                Err(EntityError::Mismatch) => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Ok(item) => (me.f)(item, cx),
            }
        }
    }
}

#[must_use = "Future does nothing unless polled"]
pub struct TryPollView<'a, Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<fn() -> &'a World>,
}

impl<Q, F, R> Future for TryPollView<'_, Q, F>
where
    Q: SendQuery,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = Option<R>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<R>> {
        unsafe {
            let me = self.get_unchecked_mut();
            match super::flow_world_ref().try_view_one_with(me.entity, me.query) {
                Err(NoSuchEntity) => on_no_such_entity(cx),
                Ok(mut view_one) => match view_one.get_mut() {
                    None => Poll::Ready(None),
                    Some(item) => (me.f)(item, cx).map(Some),
                },
            }
        }
    }
}

#[must_use = "Future does nothing unless polled"]
pub struct TryPollViewMut<'a, Q, F> {
    entity: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<fn() -> &'a mut World>,
}

impl<Q, F, R> Future for TryPollViewMut<'_, Q, F>
where
    Q: SendQuery,
    for<'a> F: FnMut(QueryItem<'a, Q>, &mut Context) -> Poll<R>,
{
    type Output = Option<R>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<R>> {
        unsafe {
            let me = self.get_unchecked_mut();
            match super::flow_world_mut().get_with(me.entity, me.query) {
                Err(EntityError::NoSuchEntity) => on_no_such_entity(cx),
                Err(EntityError::Mismatch) => Poll::Ready(None),
                Ok(item) => (me.f)(item, cx).map(Some),
            }
        }
    }
}

fn on_no_such_entity<T>(cx: &mut Context) -> Poll<T> {
    // Entity is removed.
    // Wake and pending.
    // To jump out of the future
    // and drop it on next flow run.
    cx.waker().wake_by_ref();
    Poll::Pending
}

#[cold]
#[inline(never)]
fn entity_not_alive() -> ! {
    panic!("Entity is not alive");
}
