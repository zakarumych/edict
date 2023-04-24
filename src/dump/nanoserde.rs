//! Serialization support for the entire [`World`](crate::world::World).
//!
//! This module provides the [`ComponentDump`](ComponentDump) trait, which can be implemented for

use core::convert::Infallible;

pub use nanoserde::{DeBin, DeBinErr, SerBin};

use crate::{action::ActionEncoder, component::Component, query::ImmutableQuery};

use super::{
    DumpSet, DumpSlot, Dumper, EntityDump, LoadSet, LoadSlot, Loader, Mark, WorldDump, WorldLoad,
};

/// Dumps world using [`nanoserde::SerBin`].
pub struct DumperBin<'a>(pub &'a mut Vec<u8>);

/// Loads world using [`nanoserde::DeBin`].
pub struct LoaderBin<'a> {
    offset: usize,
    buf: &'a [u8],
}

impl<'a> LoaderBin<'a> {
    /// Creates new bincode loader from buffer.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { offset: 0, buf }
    }
}

macro_rules! dumper {
    () => {};
    ($($a:ident)+) => {
        #[allow(non_snake_case)]
        impl<'a $(, $a)+> Dumper<($($a,)+)> for DumperBin<'a>
        where
            $($a: SerBin + Sync + 'static,)+
        {
            type Error = Infallible;
            fn dump(&mut self, entity: EntityDump, slots: ($(DumpSlot<'_, $a>,)+)) -> Result<(), Infallible> {
                entity.0.ser_bin(self.0);
                let ($($a,)+) = slots;
                $(
                    match $a {
                        DumpSlot::Skipped => {}
                        DumpSlot::Component($a) => $a.ser_bin(self.0),
                    }
                )+
                Ok(())
            }
        }

        impl<'a $(, $a)+, Fi> SerBin for WorldDump<'a, ($($a,)+), Fi>
        where
            Fi: ImmutableQuery + Copy,
            $($a: SerBin + Sync + 'static,)+
        {
            fn ser_bin(&self, buf: &mut Vec<u8>) {
                self.dump_bin(buf);
            }
        }

        impl<'a $(, $a)+, Fi> WorldDump<'a, ($($a,)+), Fi>
        where
            Fi: ImmutableQuery + Copy,
            $($a: SerBin + Sync + 'static,)+
        {
            fn dump_bin(&self, buf: &mut Vec<u8>) {
                let result = <($($a,)+) as DumpSet>::dump_world(self.world, self.filter, self.epoch, &mut DumperBin(buf));
                match result {
                    Ok(()) => {}
                    Err(never) => match never {},
                }
            }
        }

        #[allow(non_snake_case)]
        impl<'a $(, $a)+> Loader<($($a,)+)> for LoaderBin<'a>
        where
            $($a: DeBin + Component + Send + 'static,)+
        {
            /// Type of possible errors that can occur during deserialization.
            type Error = DeBinErr;

            /// Returns next entity to load.
            fn next(&mut self) -> Result<Option<EntityDump>, DeBinErr> {
                if self.offset == self.buf.len() {
                    return Ok(None);
                }
                let idxs = <[u64;3]>::de_bin(&mut self.offset, self.buf)?;
                Ok(Some(EntityDump(idxs)))
            }

            /// Loads entity with provided component slots.
            fn load(&mut self, slots: &mut ($(LoadSlot<'_, $a>,)+)) -> Result<(), DeBinErr> {
                let ($($a,)+) = slots;
                $(
                    match $a {
                        LoadSlot::Skipped => {}
                        LoadSlot::Missing => {
                            let comp: $a = $a::de_bin(&mut self.offset, self.buf)?;
                            *$a = LoadSlot::Created(comp);
                        }
                        LoadSlot::Existing(comp) => {
                            **comp = $a::de_bin(&mut self.offset, self.buf)?;
                        }
                        LoadSlot::Created(_) => unreachable!(),
                    }
                )+
                Ok(())
            }
        }

        impl<'a $(, $a)+, Ma> WorldLoad<'a, ($($a,)+), Ma>
        where
            Ma: Mark,
            $($a: Component + DeBin + Send + 'static,)+
        {
            /// Loads world from buffer using [`nanoserde::DeBin`].
            pub fn load_bin(&self, actions: &mut ActionEncoder, buf: &[u8]) -> Result<(), DeBinErr> {
                <($($a,)+) as LoadSet>::load_world(self.world, self.marker, actions, &mut LoaderBin::new(buf))
            }
        }
    };
}

for_tuple!(dumper);

#[test]
fn test_dump() {
    use ::nanoserde::{DeBin, SerBin};

    use super::NoMark;
    use crate::{action::ActionBuffer, epoch::EpochId, world::World};

    let mut world = World::new();

    #[derive(Component, Debug, PartialEq, Eq, SerBin, DeBin)]
    struct Foo;

    #[derive(Component, Debug, PartialEq, Eq, SerBin, DeBin)]
    struct Bar(u32);

    #[derive(Component, Debug, PartialEq, Eq, SerBin, DeBin)]
    struct Baz(String);

    let foo = world.spawn((Foo,));
    let bar = world.spawn((Bar(42),));
    let baz = world.spawn((Baz("qwerty".into()),));

    let foo_bar = world.spawn((Foo, Bar(11)));
    let foo_baz = world.spawn((Foo, Baz("asdfgh".into())));
    let bar_baz = world.spawn((Bar(23), Baz("zxcvbn".into())));
    let foo_bar_baz = world.spawn((Foo, Bar(155), Baz("123456".into())));

    type Set = (Foo, Bar, Baz);

    let data = WorldDump::<Set, _>::new(&mut world, (), EpochId::start()).serialize_bin();

    let mut world2 = World::new();

    let mut buffer = ActionBuffer::new();
    let mut actions = buffer.encoder(&world2);

    WorldLoad::<Set, _>::new(&world2, NoMark)
        .load_bin(&mut actions, &data)
        .unwrap();
    buffer.execute(&mut world2);

    assert_eq!(
        world2.query_one_mut::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(foo),
        Ok((Some(&Foo), None, None))
    );

    assert_eq!(
        world2.query_one_mut::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(bar),
        Ok((None, Some(&Bar(42)), None))
    );

    assert_eq!(
        world2.query_one_mut::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(baz),
        Ok((None, None, Some(&Baz("qwerty".into()))))
    );

    assert_eq!(
        world2.query_one_mut::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(foo_bar),
        Ok((Some(&Foo), Some(&Bar(11)), None))
    );

    assert_eq!(
        world2.query_one_mut::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(foo_baz),
        Ok((Some(&Foo), None, Some(&Baz("asdfgh".into()))))
    );

    assert_eq!(
        world2.query_one_mut::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(bar_baz),
        Ok((None, Some(&Bar(23)), Some(&Baz("zxcvbn".into()))))
    );

    assert_eq!(
        world2.query_one_mut::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(foo_bar_baz),
        Ok((Some(&Foo), Some(&Bar(155)), Some(&Baz("123456".into()))))
    );
}
