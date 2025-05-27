use core::{
    any::{type_name, TypeId},
    marker::PhantomData,
    ptr::NonNull,
};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    query::SendQuery,
    system::ActionBufferQueue,
    view::{NonExtensible, RuntimeBorrowState, StaticallyBorrowed, View, ViewCell, ViewValue},
    world::World,
    Access,
};

#[cfg(debug_assertions)]
use crate::view::{acquire, release};

use super::{FnArg, FnArgState};

/// Query suitable for [`View`] args for function-systems.
#[diagnostic::on_unimplemented(
    label = "`{Self}` cannot be used as query in function-system argument"
)]
pub trait QueryArg: SendQuery {
    /// Creates new query state.
    fn new() -> Self;

    /// Hook called before function-system runs to update the state.
    #[inline]
    fn before(&mut self, world: &World) {
        let _ = world;
    }

    /// Hook called after function-system runs to update the state.
    #[inline]
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

impl<'a, Q, F> FnArg for ViewValue<'a, Q, F, RuntimeBorrowState, NonExtensible>
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
    type Arg<'a> = ViewValue<'a, Q, F, RuntimeBorrowState, NonExtensible>;

    #[inline]
    fn new() -> Self {
        ViewState {
            query: Q::new(),
            filter: F::new(),
            marker: PhantomData,
        }
    }

    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.query.visit_archetype(archetype) && self.filter.visit_archetype(archetype)
    }

    #[inline]
    fn borrows_components_at_runtime(&self) -> bool {
        true
    }

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Option<Access> {
        let q = self
            .query
            .component_access(comp)
            .unwrap_or(Some(Access::Write));
        let f = self
            .filter
            .component_access(comp)
            .unwrap_or(Some(Access::Write));

        match (q, f) {
            (None, one) | (one, None) => one,
            (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
            (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write)) => {
                // This view uses runtime borrow, so conflict can be resolved at runtime.
                Some(Access::Write)
            }
        }
    }

    #[inline]
    fn resource_type_access(&self, _ty: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> ViewCell<'a, Q, F> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        self.query.before(world);
        self.filter.before(world);

        // Safety: Declares access for these queries.
        unsafe {
            ViewValue::new_unchecked(
                world,
                self.query,
                self.filter,
                RuntimeBorrowState::new(),
                NonExtensible,
            )
        }
    }

    #[inline]
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

impl<'a, Q, F> FnArg for ViewValue<'a, Q, F, StaticallyBorrowed, NonExtensible>
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
    type Arg<'a> = ViewValue<'a, Q, F, StaticallyBorrowed, NonExtensible>;

    #[inline]
    fn new() -> Self {
        ViewState {
            query: Q::new(),
            filter: F::new(),
            marker: PhantomData,
        }
    }

    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.query.visit_archetype(archetype) && self.filter.visit_archetype(archetype)
    }

    #[inline]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Option<Access> {
        let Ok(q) = self.query.component_access(comp) else {
            mutable_alias_in_view(comp.name(), type_name::<Self>());
        };
        let Ok(f) = self.filter.component_access(comp) else {
            mutable_alias_in_view(comp.name(), type_name::<Self>());
        };

        match (q, f) {
            (None, one) | (one, None) => one,
            (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
            (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write)) => {
                mutable_alias_in_view(comp.name(), type_name::<Self>());
            }
        }
    }

    #[inline]
    fn resource_type_access(&self, _ty: TypeId) -> Option<Access> {
        None
    }

    #[inline]
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
        unsafe {
            ViewValue::new_unchecked(
                world,
                self.query,
                self.filter,
                StaticallyBorrowed,
                NonExtensible,
            )
        }
    }

    #[inline]
    unsafe fn flush_unchecked(
        &mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };

        #[cfg(debug_assertions)]
        release(self.query, self.filter, world.archetypes());

        self.query.after(world);
        self.filter.after(world);
    }
}

#[test]
fn test_system() {
    fn foo(_: ViewCell<&u32>) {}
    fn is_system<M, T: super::IntoSystem<M>>(_: T) {}
    is_system(foo);
}

#[inline(never)]
#[cold]
fn mutable_alias_in_view(comp: &str, system: &str) -> ! {
    panic!("Mutable alias of `{comp}` in a view in system `{system}`");
}
