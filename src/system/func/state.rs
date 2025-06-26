use core::{
    any::TypeId,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
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
/// [`ResMut`]: crate::resources::ResMut
/// [`System`]: crate::system::System
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

    #[inline]
    fn deref(&self) -> &T {
        self.value
    }
}

impl<T> DerefMut for State<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.value
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

    #[inline]
    fn new() -> Self {
        Self::default()
    }

    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        None
    }

    #[inline]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    #[inline]
    fn component_access(&self, _comp: &ComponentInfo) -> Option<Access> {
        None
    }

    #[inline]
    fn resource_type_access(&self, _ty: TypeId) -> Option<Access> {
        None
    }

    #[inline]
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
