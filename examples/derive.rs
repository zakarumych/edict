use std::{
    borrow::{Borrow, BorrowMut},
    fmt::Debug,
};

use edict::{component::Component, world::World};

#[derive(Component, Debug)]
#[edict(borrow(dyn Debug, u32, f32))]
#[edict(on_drop = |_, e, _| println!("A {e:?} dropped"))]
pub struct A {
    a: f32,
}

impl Borrow<u32> for A {
    fn borrow(&self) -> &u32 {
        &0
    }
}

impl Borrow<f32> for A {
    fn borrow(&self) -> &f32 {
        &self.a
    }
}

impl BorrowMut<f32> for A {
    fn borrow_mut(&mut self) -> &mut f32 {
        &mut self.a
    }
}

fn main() {
    let mut world = World::new();
    let a = world.spawn((A { a: 1.0 },)).id();

    assert_eq!(
        world
            .new_view()
            .borrow_any::<&(dyn Debug + Sync)>()
            .into_iter()
            .count(),
        1
    );

    assert_eq!(
        world
            .new_view()
            .borrow_any::<&(dyn Debug + Send + Sync)>()
            .into_iter()
            .count(),
        1
    );

    assert_eq!(
        world
            .new_view()
            .borrow_any_mut::<dyn Debug + Send>()
            .into_iter()
            .count(),
        1
    );

    assert_eq!(
        world
            .new_view()
            .borrow_any_mut::<dyn Debug + Send + Sync>()
            .into_iter()
            .count(),
        1
    );

    assert_eq!(world.new_view().borrow_any::<u32>().into_iter().count(), 1);

    assert_eq!(
        world.new_view().borrow_any_mut::<u32>().into_iter().count(),
        0
    );

    assert_eq!(world.new_view().borrow_any::<f32>().into_iter().count(), 1);

    assert_eq!(
        world.new_view().borrow_any_mut::<f32>().into_iter().count(),
        1
    );

    assert_eq!(world.despawn(a), Ok(()));
}
