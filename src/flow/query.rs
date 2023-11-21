use core::{cell::Cell, marker::PhantomData, ptr::NonNull};

use crate::{
    entity::{EntityId, Location},
    query::{Query, QueryItem},
    world::WorldLocal,
};

/// World pointer that is updated when flow is polled.
/// It can be used to borrow world between awaits.
pub struct FlowQuery<'a, Q> {
    /// Pointer to pointer to World.
    /// The outer pointer points to the pinned pointer value
    /// that is updated when flow is polled.
    ///
    /// Inner pointer is valid between yields.
    world_location: &'a Cell<(NonNull<WorldLocal>, Location)>,
    id: EntityId,
    query: Q,
    marker: PhantomData<&'a ()>,
}

unsafe impl Send for FlowQuery<'_> {}

impl<Q> FlowQuery<'_, Q>
where
    Q: Query,
{
    /// Returns world reference that can be used to access world data.
    /// Methods that does not return references to world data can be used directly.
    /// Others need to use `WorldRef::sync` to enter non-async context.
    // SOUNDNESS BUG - FlowEntity could be sent to another thread using scoped threads
    // and access world ouside future polling.
    pub fn fetch(&mut self) -> QueryItem<'_, Q> {
        let (mut world, loc) = self.world_location.get();
        let world = unsafe { world.as_mut() };
        unsafe { EntityRef::from_parts(self.id, loc, world.local()) }
    }

    #[doc(hidden)]
    pub fn reborrow(&mut self) -> FlowEntity<'_> {
        FlowEntity {
            world_location: self.world_location,
            id: self.id,
            marker: PhantomData,
        }
    }
}

/// This trait is used to ensure that function can be called with
/// `FlowWorld` with any lifetime.
#[doc(hidden)]
pub trait FlowEntityFnG<'a> {}

impl<'a, F, Fut> FlowEntityFnG<'a> for F
where
    F: FnOnce(FlowEntity<'a>) -> Fut,
    Fut: Future<Output = ()> + Send + 'a,
{
}

#[doc(hidden)]
pub fn insert_entity_flow<F, Fut>(id: EntityId, world: &mut World, init: F)
where
    F: FnOnce(FlowEntity<'static>) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let Some(loc) = world.entities().get_location(id) else {
        return;
    };

    let mut new_flow_task: NewFlowTask<FutureFlow<Fut>> = Arc::new(MaybeUninit::uninit());
    let new_flow_task_mut = Arc::get_mut(&mut new_flow_task).unwrap();

    unsafe {
        let flow_ptr =
            addr_of_mut!((*new_flow_task_mut.as_mut_ptr()).flow).cast::<FutureFlow<Fut>>();
        let id_ptr = addr_of_mut!((*flow_ptr).id);
        id_ptr.write(id);
        let world_location_ptr = addr_of_mut!((*flow_ptr).world_location);
        world_location_ptr.write(Cell::new((NonNull::from(world.local()), loc)));

        let flow_world = FlowEntity {
            world_location: &*world_location_ptr,
            id,
            marker: PhantomData,
        };

        let fut = init(flow_world);
        let fut_ptr = addr_of_mut!((*flow_ptr).fut);
        fut_ptr.write(fut);
    }

    world
        .with_default_resource::<NewFlows>()
        .typed_new_flows()
        .array
        .push(new_flow_task);
}

impl<F, Fut> FlowEntityFn for F
where
    F: for<'a> FlowEntityFnG<'a>,
    F: FnOnce(FlowEntity<'static>) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn insert_into(self, id: EntityId, world: &mut World) {
        insert_entity_flow(id, world, |world| (self)(world));
    }
}

pub struct EntityClosure<F>(pub F);

impl<F> FlowEntityFn for EntityClosure<F>
where
    F: FnOnce(EntityId, &mut World),
{
    fn insert_into(self, id: EntityId, world: &mut World) {
        (self.0)(id, world);
    }
}

/// Converts closure syntax to entity flow fn.
///
/// There's limitation that makes following `|world: FlowWorld<'_>| async move { /*use world*/ }`
/// to be noncompilable.
///
/// On nightly it is possible to use `async move |world: FlowWorld<'_>| { /*use world*/ }`
/// But this syntax is not stable yet and edict avoids requiring too many nighty features.
///
/// This macro is a workaround for this limitation.
#[macro_export]
macro_rules! flow_closure_on {
    (|mut $entity:ident $(: $FlowEntity:ty)?| -> $ret:ty $code:block) => {
        $crate::private::EntityClosure(|id: $crate::entity::EntityId, world: &mut $crate::world::World| {
            $crate::private::insert_entity_flow(
                id,
                world,
                |mut world: $crate::flow::FlowEntity<'static>| async move {
                    let mut $entity $(: $FlowEntity)? = world.reborrow();
                    let res: $ret = { $code };
                    res
                },
            )
        })
    };
    (|mut $entity:ident $(: $FlowEntity:ty)?| $code:expr) => {
        $crate::private::EntityClosure(|id: $crate::entity::EntityId, world: &mut $crate::world::World| {
            $crate::private::insert_entity_flow(
                id,
                world,
                |mut world: $crate::flow::FlowEntity<'static>| async move {
                    let mut $entity $(: $FlowEntity)? = world.reborrow();
                    $code
                },
            )
        })
    };
}
