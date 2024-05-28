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
    Entity, EntityError, NoSuchEntity,
};

use super::{flow_world, Flow, FlowWorld};

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

        if !flow_world().is_alive(this.id) {
            // Terminate flow if entity is removed.
            return Poll::Ready(());
        };

        let fut = unsafe { Pin::new_unchecked(&mut this.fut) };
        let poll = fut.poll(cx);

        let world = flow_world();

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
    pub unsafe fn make(id: EntityId) -> Self {
        FlowEntity {
            id,
            marker: PhantomData,
        }
    }

    pub fn id(&self) -> EntityId {
        self.id
    }

    #[doc(hidden)]
    pub fn reborrow(&mut self) -> FlowEntity<'_> {
        FlowEntity {
            id: self.id,
            marker: PhantomData,
        }
    }

    pub fn world(&mut self) -> FlowWorld<'_> {
        FlowWorld {
            marker: PhantomData,
        }
    }

    #[inline(always)]
    pub fn get_ref(&mut self) -> EntityRef<'_> {
        unsafe { flow_world().entity(self.id).unwrap() }
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    pub fn poll_ref<F, Q, R>(&mut self, f: F) -> PollRef<'_, F>
    where
        F: FnMut(EntityRef, &mut Context) -> Poll<R>,
    {
        PollRef {
            id: self.id,
            f,
            marker: PhantomData,
        }
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    /// Waits until query is satisfied.
    pub fn poll_view<Q, F, R>(&self, f: F) -> PollView<'_, Q::Query, F>
    where
        Q: DefaultQuery,
        Q::Query: ImmutableQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        PollView {
            id: self.id,
            f,
            query: Q::default_query(),
            marker: PhantomData,
        }
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    /// Waits until query is satisfied.
    pub fn poll_view_mut<Q, F, R>(&mut self, f: F) -> PollView<'_, Q::Query, F>
    where
        Q: DefaultQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        PollView {
            id: self.id,
            f,
            query: Q::default_query(),
            marker: PhantomData,
        }
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    /// If query is not satisfied, returns [`Poll::Ready(None)`].
    pub fn try_poll_view<Q, F, R>(&self, f: F) -> TryPollView<'_, Q::Query, F>
    where
        Q: DefaultQuery,
        Q::Query: ImmutableQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        TryPollView {
            id: self.id,
            f,
            query: Q::default_query(),
            marker: PhantomData,
        }
    }

    /// Polls the world until closure returns [`Poll::Ready`].
    /// If query is not satisfied, returns [`Poll::Ready(None)`].
    pub fn try_poll_view_mut<Q, F, R>(&mut self, f: F) -> TryPollView<'_, Q::Query, F>
    where
        Q: DefaultQuery,
        F: FnMut(QueryItem<Q>, &mut Context) -> Poll<R>,
    {
        TryPollView {
            id: self.id,
            f,
            query: Q::default_query(),
            marker: PhantomData,
        }
    }

    /// Unsafely fetches a component from the entity.
    ///
    /// It is intended to be used to construct safe API on top of it.
    ///
    /// # Safety
    ///
    /// Caller must ensure that this method won't create aliased mutable references.
    #[inline(always)]
    pub unsafe fn fetch<T>(&self) -> Result<&T, EntityError>
    where
        T: Sync + 'static,
    {
        unsafe { flow_world().get_unchecked::<&T>(self.id) }
    }

    /// Unsafely fetches component from the entity.
    ///
    /// It is intended to be used to construct safe API on top of it.
    ///
    /// # Safety
    ///
    /// Caller must ensure that this method won't create aliased mutable references.
    #[inline(always)]
    pub unsafe fn fetch_mut<T>(&self) -> Result<&mut T, EntityError>
    where
        T: Send + 'static,
    {
        unsafe { flow_world().get_unchecked::<&mut T>(self.id) }
    }

    /// Queries component from the entity.
    ///
    /// Returns clone of the component value.
    #[inline(always)]
    pub fn get_cloned<T>(&self) -> Result<T, EntityError>
    where
        T: Clone + Sync + 'static,
    {
        unsafe {
            let c: &T = flow_world().get_unchecked::<&T>(self.id)?;
            Ok(c.clone())
        }
    }

    /// Queries component from the entity.
    ///
    /// Returns copy of the component value.
    #[inline(always)]
    pub fn get_copied<T>(&self) -> Result<T, EntityError>
    where
        T: Copy + Sync + 'static,
    {
        unsafe {
            let c: &T = flow_world().get_unchecked::<&T>(self.id)?;
            Ok(*c)
        }
    }

    /// Sets new value for the entity component.
    ///
    /// Returns error if entity does not have component of specified type.
    #[inline(always)]
    pub fn set<T>(&self, value: T) -> Result<(), EntityError>
    where
        T: Send + 'static,
    {
        unsafe {
            let c: &mut T = flow_world().get_unchecked::<&mut T>(self.id)?;
            *c = value;
            Ok(())
        }
    }

    /// Insert a component to the entity.
    #[inline(always)]
    pub fn insert<T>(&self, component: T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        unsafe { flow_world().insert(self.id, component) }
    }

    /// Attempts to inserts component to the entity.
    ///
    /// If entity already had component of that type,
    /// old component value is replaced with new one.
    /// Otherwise new component is added to the entity.
    #[inline(always)]
    pub fn insert_external<T>(&self, component: T) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        unsafe { flow_world().insert_external(self.id, component) }
    }

    /// Inserts a component to the entity if it does not have one.
    #[inline(always)]
    pub fn with<T>(&self, component: impl FnOnce() -> T) -> Result<(), NoSuchEntity>
    where
        T: Component,
    {
        unsafe {
            flow_world().with(self.id, component)?;
        }
        Ok(())
    }

    /// Attempts to insert a component to the entity if it does not have one.
    #[inline(always)]
    pub fn with_external<T>(&self, component: impl FnOnce() -> T) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        unsafe {
            flow_world().with_external(self.id, component)?;
        }
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
    pub fn insert_bundle<B>(&mut self, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicComponentBundle,
    {
        unsafe { flow_world().insert_bundle(self.id, bundle) }
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
    pub fn insert_external_bundle<B>(&self, bundle: B) -> Result<(), NoSuchEntity>
    where
        B: DynamicBundle,
    {
        unsafe { flow_world().insert_external_bundle(self.id, bundle) }
    }

    /// Removes a component from the entity.
    /// Returns the component if it was present.
    #[inline(always)]
    pub fn remove<T>(&self) -> Result<T, EntityError>
    where
        T: 'static,
    {
        unsafe {
            let (c, _) = flow_world().remove::<T>(self.id)?;
            match c {
                None => Err(EntityError::Mismatch),
                Some(c) => Ok(c),
            }
        }
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop<T>(&self) -> Result<(), NoSuchEntity>
    where
        T: 'static,
    {
        unsafe { flow_world().drop::<T>(self.id) }
    }

    /// Drops a component from the entity.
    #[inline(always)]
    pub fn drop_erased(&self, ty: TypeId) -> Result<(), NoSuchEntity> {
        unsafe { flow_world().drop_erased(self.id, ty) }
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
    pub fn drop_bundle<B>(&self) -> Result<(), NoSuchEntity>
    where
        B: Bundle,
    {
        unsafe { flow_world().drop_bundle::<B>(self.id) }
    }

    /// Despawns the referenced entity.
    #[inline(always)]
    pub fn despawn(&self) -> Result<(), NoSuchEntity> {
        unsafe { flow_world().despawn(self.id) }
    }

    /// Checks if entity has component of specified type.
    ///
    /// If entity is not alive, fails with `Err(NoSuchEntity)`.
    #[inline(always)]
    pub fn has_component<T: 'static>(&self) -> Result<bool, NoSuchEntity> {
        unsafe { flow_world().try_has_component::<T>(self.id) }
    }

    /// Spawns a new flow for the entity.
    pub fn spawn<F>(&self, f: F)
    where
        F: for<'a> EntityFlowFn<'a> + 'static,
    {
        super::spawn_local_for(unsafe { flow_world() }.local(), self.id, f);
    }
}

#[must_use = "Future does nothing unless polled"]
pub struct PollRef<'a, F> {
    id: EntityId,
    f: F,
    marker: PhantomData<&'a ()>,
}

impl<F, R> Future for PollRef<'_, F>
where
    F: FnMut(EntityRef, &mut Context) -> Poll<R>,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<R> {
        unsafe {
            let me = self.get_unchecked_mut();
            let e = flow_world().entity(me.id).unwrap();
            (me.f)(e, cx)
        }
    }
}

#[must_use = "Future does nothing unless polled"]
pub struct PollView<'a, Q, F> {
    id: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<&'a ()>,
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
            let mut view_one = flow_world().try_view_one_with(me.id, me.query).unwrap();

            let Some(item) = view_one.get_mut() else {
                return Poll::Pending;
            };
            (me.f)(item, cx)
        }
    }
}

#[must_use = "Future does nothing unless polled"]
pub struct TryPollView<'a, Q, F> {
    id: EntityId,
    query: Q,
    f: F,
    marker: PhantomData<&'a ()>,
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
            let mut view_one = flow_world().try_view_one_with(me.id, me.query).unwrap();

            let Some(item) = view_one.get_mut() else {
                return Poll::Ready(None);
            };
            (me.f)(item, cx).map(Some)
        }
    }
}
