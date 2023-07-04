use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    query::{merge_access, Access, Query},
    system::ActionQueue,
    view::{acquire, release, RuntimeBorrowState, StaticallyBorrowed, View, ViewCell, ViewValue},
    world::World,
};

use super::{FnArg, FnArgState};

/// Query suitable for [`View`] args for function-systems.
pub trait QueryArg: Query {
    /// Creates new query state.
    fn new() -> Self;

    /// Hook called before function-system runs to update the state.
    #[inline(always)]
    fn before(&mut self, world: &World) {
        drop(world);
    }

    /// Hook called after function-system runs to update the state.
    #[inline(always)]
    fn after(&mut self, world: &World) {
        drop(world);
    }
}

/// State type used by corresponding [`QueryRef`].
#[derive(Default)]
pub struct ViewState<Q, F, B> {
    query: Q,
    filter: F,
    marker: PhantomData<B>,
}

impl<'a, Q, F> FnArg for ViewValue<'a, Q, F, RuntimeBorrowState>
where
    Q: QueryArg,
    F: QueryArg,
{
    type State = ViewState<Q, F, RuntimeBorrowState>;
}

unsafe impl<Q, F> FnArgState for ViewState<Q, F, RuntimeBorrowState>
where
    Q: QueryArg,
    F: QueryArg,
{
    type Arg<'a> = ViewCell<'a, Q, F>;

    #[inline(always)]
    fn new() -> Self {
        ViewState {
            query: Q::new(),
            filter: F::new(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        false
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.query.visit_archetype(archetype) && self.filter.visit_archetype(archetype)
    }

    #[inline(always)]
    fn access_component(&self, id: TypeId) -> Option<Access> {
        merge_access::<Self>(self.query.access(id), self.filter.access(id))
    }

    #[inline(always)]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> ViewCell<'a, Q, F> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        self.query.before(world);
        self.filter.before(world);
        ViewValue::new_cell(world, self.query, self.filter)
    }

    #[inline(always)]
    unsafe fn flush_unchecked(&mut self, world: NonNull<World>, _queue: &mut dyn ActionQueue) {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        self.query.after(world);
        self.filter.after(world);
    }
}

impl<'a, Q, F> FnArg for ViewValue<'a, Q, F, StaticallyBorrowed>
where
    Q: QueryArg,
    F: QueryArg,
{
    type State = ViewState<Q, F, StaticallyBorrowed>;
}

unsafe impl<Q, F> FnArgState for ViewState<Q, F, StaticallyBorrowed>
where
    Q: QueryArg,
    F: QueryArg,
{
    type Arg<'a> = ViewValue<'a, Q, F, StaticallyBorrowed>;

    #[inline(always)]
    fn new() -> Self {
        ViewState {
            query: Q::new(),
            filter: F::new(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        false
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.query.visit_archetype(archetype) && self.filter.visit_archetype(archetype)
    }

    #[inline(always)]
    fn access_component(&self, id: TypeId) -> Option<Access> {
        merge_access::<Self>(self.query.access(id), self.filter.access(id))
    }

    #[inline(always)]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> View<'a, Q, F> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };

        #[cfg(debug_assertions)]
        acquire(&self.query, &self.filter, world.archetypes());

        self.query.before(world);
        self.filter.before(world);

        // Safety: Declares access for these queries.
        unsafe { ViewValue::new_unchecked(world, self.query, self.filter) }
    }

    #[inline(always)]
    unsafe fn flush_unchecked(&mut self, world: NonNull<World>, _queue: &mut dyn ActionQueue) {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        self.query.after(world);
        self.filter.after(world);

        #[cfg(debug_assertions)]
        release(&self.query, &self.filter, world.archetypes());
    }
}

#[test]
fn test_system() {
    fn foo(_: ViewCell<&u32>) {}
}
