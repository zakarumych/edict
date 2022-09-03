use core::{
    any::TypeId,
    ops::{Deref, DerefMut},
};

use crate::{archetype::Archetype, query::Access, system::ActionQueue, world::World};

use super::{FnArg, FnArgCache, FnArgGet, FromWorld};

/// Bare state for function systems.
///
/// The difference between [`ResMut`] and [`State`]
/// is that [`State`] is not stored in the [`World`]
/// and is not shared between [`System`]s.
/// Instead, each [`System`] has its own cached [`State`]
/// which is automatically initialized with [`FromWorld`] (super-trait of [`Default`])
/// on first access.
#[repr(transparent)]
pub struct State<'a, T> {
    value: &'a mut T,
}

/// [`FnArgFetch`] for [`State`] argument.
pub struct StateCache<T> {
    value: Option<T>,
}

impl<T> Default for StateCache<T> {
    fn default() -> Self {
        StateCache { value: None }
    }
}

impl<T> Deref for State<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for State<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> FnArg for State<'_, T>
where
    T: FromWorld + Send + 'static,
{
    type Cache = StateCache<T>;
}

unsafe impl<'a, T> FnArgGet<'a> for StateCache<T>
where
    T: FromWorld + Send + 'static,
{
    type Arg = State<'a, T>;

    unsafe fn get_unchecked(
        &'a mut self,
        world: &'_ World,
        _queue: &mut dyn ActionQueue,
    ) -> State<'a, T> {
        let value = self.value.get_or_insert_with(|| T::from_world(world));
        State { value }
    }
}

impl<T> FnArgCache for StateCache<T>
where
    T: FromWorld + Send + 'static,
{
    fn is_local(&self) -> bool {
        false
    }

    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }
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
        system.run_unchecked(&world, &mut encoders);
        system.run_unchecked(&world, &mut encoders);
        system.run_unchecked(&world, &mut encoders);
        system.run_unchecked(&world, &mut encoders);
    }
}
