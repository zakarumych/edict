use edict::prelude::*;

struct Foo;

impl Drop for Foo {
    fn drop(&mut self) {
        println!("Dropping Foo");
    }
}

struct Bar;

impl Drop for Bar {
    fn drop(&mut self) {
        println!("Dropping Bar");
    }
}

struct Baz;

impl Drop for Baz {
    fn drop(&mut self) {
        println!("Dropping Baz");
    }
}

fn main() {
    let mut world = World::new();

    let e = world.spawn((Foo,));

    world.insert(&e, Bar);
    world.insert(&e, Bar);
    world.insert_bundle(&e, (Baz,));

    println!("Maintain");
    world.maintain();

    println!("Remove Foo");
    world.remove::<Foo>(&e).unwrap();

    println!("Maintain");
    world.maintain();

    println!("Drop entity");
    drop(e);

    println!("Maintain");
    world.maintain();
}
