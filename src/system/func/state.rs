use core::{
    any::TypeId,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{archetype::Archetype, query::Access, system::ActionQueue, world::World};

use super::{FnArg, FnArgCache, FnArgGet};

/// Bare state for function systems.
///
/// The difference between [`ResMut`] and [`State`]
/// is that [`State`] is not stored in the [`World`]
/// and is not shared between [`System`]s.
/// Instead, each [`System`] has its own cached [`State`]
/// which is automatically initialized with [`Default`]
/// on first access.
#[repr(transparent)]
pub struct State<'a, T> {
    value: &'a mut T,
}

/// [`FnArgFetch`] for [`State`] argument.
#[derive(Default)]
pub struct StateCache<T> {
    value: T,
}

impl<T> Deref for State<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for State<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> FnArg for State<'_, T>
where
    T: Default + Send + 'static,
{
    type Cache = StateCache<T>;
}

unsafe impl<'a, T> FnArgGet<'a> for StateCache<T>
where
    T: Default + Send + 'static,
{
    type Arg = State<'a, T>;

    #[inline]
    unsafe fn get_unchecked(
        &'a mut self,
        _world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> State<'a, T> {
        State {
            value: &mut self.value,
        }
    }
}

impl<T> FnArgCache for StateCache<T>
where
    T: Default + Send + 'static,
{
    #[inline]
    fn is_local(&self) -> bool {
        false
    }

    #[inline]
    fn world_access(&self) -> Option<Access> {
        None
    }

    #[inline]
    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    #[inline]
    fn access_resource(&self, _id: TypeId) -> Option<Access> {
        None
    }
}

#[test]
fn test_state_system() {
    use super::{IntoSystem, System};

    fn bar(mut state: State<u32>) {
        *state = *state + 1;
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
