use core::{
    any::{type_name, TypeId},
    marker::PhantomData,
    ptr::NonNull,
};

use crate::{
    archetype::Archetype,
    query::SendQuery,
    system::ActionBufferQueue,
    view::{acquire, release, RuntimeBorrowState, StaticallyBorrowed, View, ViewCell, ViewValue},
    world::World,
    Access,
};

use super::{FnArg, FnArgState};

/// Query suitable for [`View`] args for function-systems.
pub trait QueryArg: SendQuery {
    /// Creates new query state.
    fn new() -> Self;

    /// Hook called before function-system runs to update the state.
    #[inline(always)]
    fn before(&mut self, world: &World) {
        let _ = world;
    }

    /// Hook called after function-system runs to update the state.
    #[inline(always)]
    fn after(&mut self, world: &World) {
        let _ = world;
    }
}

/// State type used by corresponding [`View`].
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
    type Arg<'a> = ViewValue<'a, Q, F, RuntimeBorrowState>;

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
    fn borrows_components_at_runtime(&self) -> bool {
        true
    }

    #[inline(always)]
    fn component_type_access(&self, ty: TypeId) -> Option<Access> {
        let q = self
            .query
            .component_type_access(ty)
            .unwrap_or(Some(Access::Write));
        let f = self
            .filter
            .component_type_access(ty)
            .unwrap_or(Some(Access::Write));

        match (q, f) {
            (None, one) | (one, None) => one,
            (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
            _ => {
                // This view uses runtime borrow, so conflict can be resolved at runtime.
                Some(Access::Write)
            }
        }
    }

    #[inline(always)]
    fn resource_type_access(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> ViewCell<'a, Q, F> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        self.query.before(world);
        self.filter.before(world);
        ViewValue::new_cell(world, self.query, self.filter)
    }

    #[inline(always)]
    unsafe fn flush_unchecked(
        &mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) {
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
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    #[inline(always)]
    fn component_type_access(&self, ty: TypeId) -> Option<Access> {
        let Ok(q) = self.query.component_type_access(ty) else {
            panic!("Mutable alias in query of `{}`", type_name::<Self>());
        };
        let Ok(f) = self.filter.component_type_access(ty) else {
            panic!("Mutable alias in filter of `{}`", type_name::<Self>());
        };

        match (q, f) {
            (None, one) | (one, None) => one,
            (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
            _ => {
                panic!(
                    "Conflicting query and filter in `{}`.
                        A component is aliased mutably.",
                    core::any::type_name::<Self>()
                );
            }
        }
    }

    #[inline(always)]
    fn resource_type_access(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> View<'a, Q, F> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };

        #[cfg(debug_assertions)]
        acquire(self.query, self.filter, world.archetypes());

        self.query.before(world);
        self.filter.before(world);

        // Safety: Declares access for these queries.
        unsafe { ViewValue::new_unchecked(world, self.query, self.filter) }
    }

    #[inline(always)]
    unsafe fn flush_unchecked(
        &mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        self.query.after(world);
        self.filter.after(world);

        #[cfg(debug_assertions)]
        release(self.query, self.filter, world.archetypes());
    }
}

#[test]
fn test_system() {
    fn foo(_: ViewCell<&u32>) {}
    fn is_system<M, T: super::IntoSystem<M>>(_: T) {}
    is_system(foo);
}
