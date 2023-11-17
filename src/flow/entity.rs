use core::{
    any::TypeId,
    future::Future,
    marker::PhantomData,
    mem::MaybeUninit,
    pin::Pin,
    ptr::addr_of_mut,
    task::{Context, Poll, Waker},
};

use alloc::sync::Arc;
use smallvec::SmallVec;

use crate::{
    action::LocalActionEncoder,
    bundle::{Bundle, DynamicBundle, DynamicComponentBundle},
    component::Component,
    entity::{EntityId, EntityRef},
    world::World,
    EntityError, NoSuchEntity,
};

use super::{flow_world, Flow, NewFlowTask, NewFlows};

/// Entity reference usable in flows.
///
/// It can be used to access entity's components,
/// insert and remove components.
#[derive(Clone, Copy, Debug)]
pub struct FlowEntity<'a> {
    id: EntityId,
    marker: PhantomData<&'a mut World>,
}

unsafe impl Send for FlowEntity<'_> {}
unsafe impl Sync for FlowEntity<'_> {}

/// Future wrapped to be used as a flow.
struct FutureFlow<F> {
    id: EntityId,
    fut: F,
}

impl<F> Flow for FutureFlow<F>
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
        let Some(loc) = world.entities().get_location(this.id) else {
            // Terminate flow if entity is removed.
            return Poll::Ready(());
        };

        let mut e = EntityRef::from_parts(this.id, loc, world.local());

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

#[doc(hidden)]
pub fn insert_entity_flow<F>(id: EntityId, world: &mut World, f: F)
where
    F: EntityFlowFn<'static>,
{
    let mut new_flow_task: NewFlowTask<FutureFlow<F::Fut>> = Arc::new(MaybeUninit::uninit());
    let new_flow_task_mut = Arc::get_mut(&mut new_flow_task).unwrap();

    unsafe {
        let flow_ptr =
            addr_of_mut!((*new_flow_task_mut.as_mut_ptr()).flow).cast::<FutureFlow<F::Fut>>();

        let fut = f.run(FlowEntity::new(id));
        let fut_ptr = addr_of_mut!((*flow_ptr).fut);
        fut_ptr.write(fut);

        let id_ptr = addr_of_mut!((*flow_ptr).id);
        id_ptr.write(id);
    }

    world
        .with_default_resource::<NewFlows>()
        .typed_new_flows()
        .array
        .push(new_flow_task);
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

pub trait EntityFlowSpawn {
    fn spawn(self, id: EntityId, world: &mut World);
}

impl<F> EntityFlowSpawn for F
where
    F: for<'a> EntityFlowFn<'a>,
{
    fn spawn(self, id: EntityId, world: &mut World) {
        insert_entity_flow(id, world, self);
    }
}

pub struct EntityClosureSpawn<F>(pub F);

impl<F> EntityFlowSpawn for EntityClosureSpawn<F>
where
    F: FnOnce(EntityId, &mut World),
{
    fn spawn(self, id: EntityId, world: &mut World) {
        (self.0)(id, world)
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
        $crate::private::EntityClosureSpawn(|id: $crate::entity::EntityId, world: &mut $crate::world::World| {
            $crate::private::insert_entity_flow(
                id,
                world,
                |entity: $crate::flow::FlowEntity<'static>| async move {
                    let $entity $(: $FlowEntity)? = entity.reborrow();
                    let res: $ret = { $code };
                    res
                },
            )
        })
    };
    (|mut $entity:ident $(: $FlowEntity:ty)?| $code:expr) => {
        $crate::private::EntityClosureSpawn(|id: $crate::entity::EntityId, world: &mut $crate::world::World| {
            $crate::private::insert_entity_flow(
                id,
                world,
                |entity: $crate::flow::FlowEntity<'static>| async move {
                    let $entity $(: $FlowEntity)? = entity.reborrow();
                    $code
                },
            )
        })
    };
}

pub fn spawn_for<F>(id: EntityId, world: &mut World, flow_fn: F)
where
    F: EntityFlowSpawn,
{
    flow_fn.spawn(id, world);
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
    pub(crate) fn new(id: EntityId) -> Self {
        FlowEntity {
            id,
            marker: PhantomData,
        }
    }

    #[doc(hidden)]
    pub fn reborrow(&self) -> FlowEntity<'_> {
        FlowEntity {
            id: self.id,
            marker: PhantomData,
        }
    }

    pub fn id(&self) -> EntityId {
        self.id
    }

    /// Unsafely fetches component from the entity.
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
    pub fn get<T>(&self) -> Result<T, EntityError>
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
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    ///
    /// let mut entity = world.spawn(());
    ///
    /// assert!(!entity.has_component::<u32>());
    /// entity.insert_external(42u32);
    /// assert!(entity.has_component::<u32>());
    /// ```
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
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.ensure_external_registered::<u32>();
    ///
    /// let mut entity = world.spawn(());
    ///
    /// assert!(!entity.has_component::<u32>());
    /// entity.with_external(|| 42u32);
    /// assert!(entity.has_component::<u32>());
    /// ```
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
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    /// let mut entity = world.spawn(());
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// entity.insert_bundle((ExampleComponent,));
    /// assert!(entity.has_component::<ExampleComponent>());
    /// ```
    #[inline(always)]
    pub fn insert_bundle<B>(&self, bundle: B) -> Result<(), NoSuchEntity>
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
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    /// let mut world = World::new();
    ///
    /// world.ensure_component_registered::<ExampleComponent>();
    /// world.ensure_external_registered::<u32>();
    ///
    /// let mut entity = world.spawn(());
    ///
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// assert!(!entity.has_component::<u32>());
    ///
    /// entity.insert_external_bundle((ExampleComponent, 42u32));
    ///
    /// assert!(entity.has_component::<ExampleComponent>());
    /// assert!(entity.has_component::<u32>());
    /// ```
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
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, ExampleComponent};
    ///
    /// struct OtherComponent;
    ///
    /// let mut world = World::new();
    /// let mut entity = world.spawn((ExampleComponent,));
    ///
    /// assert!(entity.has_component::<ExampleComponent>());
    ///
    /// entity.drop_bundle::<(ExampleComponent, OtherComponent)>();
    ///
    /// assert!(!entity.has_component::<ExampleComponent>());
    /// ```
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
}
