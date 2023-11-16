use core::{
    any::TypeId,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    archetype::Archetype,
    system::{Access, ActionBufferQueue},
    world::World,
};

use super::{FnArg, FnArgState};

/// State for function systems.
/// Value inside [`State`] is preserved between system runs.
///
/// The difference between [`ResMut`] and [`State`]
/// is that [`State`] is not stored in the [`World`]
/// and is not shared between [`System`]s.
/// Instead each [`System`] gets its own cached instance of [`State`]
/// which is automatically initialized with [`Default`]
/// on first access.
///
/// [`ResMut`]: super::res::ResMut
/// [`System`]: edict::system::System
#[repr(transparent)]
pub struct State<'a, T> {
    value: &'a mut T,
}

impl<'a, T> From<&'a mut T> for State<'a, T> {
    fn from(value: &'a mut T) -> Self {
        State { value }
    }
}

/// [`FnArgState`] for [`State`] argument.
#[derive(Default)]
#[repr(transparent)]
pub struct StateState<T> {
    value: T,
}

impl<T> Deref for State<'_, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for State<'_, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> FnArg for State<'_, T>
where
    T: Default + Send + 'static,
{
    type State = StateState<T>;
}

unsafe impl<T> FnArgState for StateState<T>
where
    T: Default + Send + 'static,
{
    type Arg<'a> = State<'a, T>;

    #[inline(always)]
    fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        false
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        None
    }

    #[inline(always)]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline(always)]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    #[inline(always)]
    fn component_type_access(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    fn resource_type_access(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        _world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> State<'a, T> {
        State {
            value: &mut self.value,
        }
    }
}

#[test]
fn test_state_system() {
    use alloc::vec::Vec;

    use super::{IntoSystem, System};

    fn bar(mut state: State<u32>) {
        *state = *state + 1;
        #[cfg(feature = "std")]
        println!("{}", *state);
    }

    let mut system = bar.into_system();

    let world = World::new();
    let mut encoders = Vec::new();

    unsafe {
        system.run_unchecked(NonNull::from(&world), &mut encoders);
        system.run_unchecked(NonNull::from(&world), &mut encoders);
        system.run_unchecked(NonNull::from(&world), &mut encoders);
        system.run_unchecked(NonNull::from(&world), &mut encoders);
    }
}
