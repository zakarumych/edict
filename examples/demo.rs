use std::time::Instant;

use edict::{
    proof::Skip,
    query::{Alt, Modifed},
    world::World,
};

struct Foo;
struct Bar;
struct Baz;

fn main() {
    let mut world = World::new();

    let e1 = world.spawn((Foo, Bar, Baz, "qwerty".to_owned()));

    let e1 = world.pin::<(Foo, Bar)>(e1);

    let (foo, bar, int) = world.get_mut::<(&mut Foo, &Bar, Option<&u32>), _>(&e1);

    for (e, Foo) in world.query_mut::<&Foo>() {
        println!("{}", e);
    }

    let mut tracks = world.tracks();

    for (e, Foo) in world.query_tracked_mut::<Modifed<&Foo>>(&mut tracks) {
        panic!("Must not be called");
    }

    let e2 = world.spawn((Foo, Bar, Baz));

    for (e, Foo) in world.query_tracked_mut::<Modifed<&Foo>>(&mut tracks) {
        println!("New component is considered mofied {}", e);
    }

    for (e, mut r) in world.query_mut::<Alt<Foo>>() {
        let foo = &mut *r; // Marks as modified
    }

    for (e, Foo) in world.query_tracked_mut::<Modifed<&Foo>>(&mut tracks) {
        println!("{}", e);
    }

    for (e, Foo) in world.query_tracked_mut::<Modifed<&Foo>>(&mut tracks) {
        panic!("Must not be called");
    }

    world.get_mut::<(&mut Foo, Skip), _>(&e1);

    for (e, Foo) in world.query_tracked_mut::<Modifed<&Foo>>(&mut tracks) {
        assert_eq!(e, e1, "Only e1 was modified");
    }

    drop(e1);
    drop(e2);

    world.maintain();
}

fn alt_speed(world: &mut World) {
    struct S1(u64);
    struct S2(u64);
    struct S3(u64);
    struct S4(u64);
    struct S5(u64);

    for _ in 0..1 << 14 {
        world.spawn((S1(1), S2(2), S3(3), S4(4), S5(5)));
    }

    let start = Instant::now();

    for _ in 0..1 << 12 {
        for (_, (mut s1, mut s2, mut s3, mut s4)) in
            world.query_mut::<(&mut S1, &mut S2, &mut S3, &mut S4)>()
        {
            let _: &mut S1 = &mut s1;
            let _: &mut S2 = &mut s2;
            let _: &mut S3 = &mut s3;
            let _: &mut S4 = &mut s4;
        }
    }

    println!("&mut S = {}ms", start.elapsed().as_secs_f32() * 1000.0);

    let start = Instant::now();

    for _ in 0..1 << 12 {
        for (_, (mut s1, mut s2, mut s3, mut s4)) in
            world.query_mut::<(Alt<S1>, Alt<S2>, Alt<S3>, Alt<S4>)>()
        {
            let _: &mut S1 = &mut s1;
            let _: &mut S2 = &mut s2;
            let _: &mut S3 = &mut s3;
            let _: &mut S4 = &mut s4;
        }
    }

    println!("Alt<S> = {}ms", start.elapsed().as_secs_f32() * 1000.0);
}
