use std::{
    any::{Any, TypeId},
    fmt::Display,
};

use edict::{component::Component, query::Entities, world::World};

#[derive(Component)]
#[edict(borrow(dyn Display))]
struct A;

impl Display for A {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("A")
    }
}

#[derive(Component)]
#[edict(borrow(dyn Display))]
struct B;

impl Display for B {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("B")
    }
}

fn main() {
    drop(());

    let mut world = World::new();

    // Spawn pair of entities.
    let _a = world.spawn((A,));
    let _b = world.spawn((B,));

    // Spawn entity with both.
    let _c = world.spawn((A, B));

    // Borrow any component that exposes `Display` trait.
    // Skips entities without such component.
    for display in world
        .new_view()
        .borrow_any_mut::<dyn Display + Send>()
        .iter()
    {
        println!("{}", display);
    }

    // Borrow component with specific `TypeId` as `Any` trait object.
    // Current behavior is to panic if component with that type id is found
    // and it doesn't exposes `Any` trait.
    for a in world
        .new_view()
        .borrow_one::<&(dyn Any + Sync)>(TypeId::of::<A>())
        .iter()
    {
        println!("{}", (a as &dyn Any).downcast_ref::<A>().unwrap());
    }

    // Borrow all components that expose `Display` trait.
    // This query yields vector of `&dyn Display` trait objects for each entity.
    // Current behavior is to skip entities with no such components.
    for (e, a) in world
        .view::<Entities>()
        .borrow_all::<&(dyn Display + Sync)>()
        .iter()
    {
        print!("{}", e);
        for a in a {
            print!(" {}", a);
        }
        println!();
    }
}
