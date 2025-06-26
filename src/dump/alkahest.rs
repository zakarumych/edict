//! World serialization with [`alkahest`].

use alkahest::{
    advanced::{slice_writer, write_field, BareFormula, Buffer, Deserializer, Sizes, SliceWriter},
    deserialize_with_size, serialize_to_vec, DeIter, Deserialize, DeserializeError, Formula, Lazy,
    Serialize,
};

use crate::{action::ActionEncoder, component::Component, query::SendImmutableQuery};

use super::{
    DumpSet, DumpSlot, Dumper, EntityDump, LoadSet, LoadSlot, Loader, Mark, WorldDump, WorldLoad,
};

/// Formula for single entity.
pub struct DumpFormula<F>(F);

impl<F> Formula for DumpFormula<F>
where
    F: Formula,
{
    const EXACT_SIZE: bool = false;
    const HEAPLESS: bool = false;
    const MAX_STACK_SIZE: Option<usize> = None;
}

impl<F> BareFormula for DumpFormula<F> where F: Formula {}

/// Formula for serializing world with set of components.
pub type WorldFormula<F> = [([u64; 3], DumpFormula<F>)];

#[allow(clippy::type_complexity)]
struct LoaderAlkahest<'de, F> {
    iter: DeIter<'de, ([u64; 3], DumpFormula<F>), ([u64; 3], Lazy<'de, DumpFormula<F>>)>,
    next: Option<Lazy<'de, DumpFormula<F>>>,
}

macro_rules! dumper {
    (,) => {};
    ($($c:ident)+, $($f:ident)+) => {
        #[allow(non_snake_case)]
        impl<'a $(, $f)+ $(, $c)+> Serialize<DumpFormula<($($f,)+)>> for ($(DumpSlot<'a, $c>,)+)
        where
            $($f: Formula, &'a $c: Serialize<$f>,)+
        {
            fn serialize<Bu>(self, sizes: &mut Sizes, mut buffer: Bu) -> Result<(), Bu::Error>
            where
                Bu: Buffer,
            {
                let ($($c,)+) = self;
                $(
                    match $c {
                        DumpSlot::Skipped => {}
                        DumpSlot::Component($c) => write_field::<$f, &$c, _>($c, sizes, buffer.reborrow(), false)?,
                    }
                )+
                Ok(())
            }

            fn size_hint(&self) -> Option<Sizes> {
                None
            }
        }

        impl<'a, Bu $(, $f)+ $(, $c)+> Dumper<($($c,)+)> for SliceWriter<'a, ([u64; 3], DumpFormula<($($f,)+)>), Bu>
        where
            Bu: Buffer + ?Sized,
            $($f: Formula, $c: Sync + 'static, for<'b> &'b $c: Serialize<$f>,)+
        {
            type Error = Bu::Error;
            fn dump(&mut self, entity: EntityDump, tuple: ($(DumpSlot<'_, $c>,)+)) -> Result<(), Bu::Error> {
                self.write_elem((entity.0, tuple))
            }
        }

        impl<'a $(, $c)+, Fi> WorldDump<'a, ($($c,)+), Fi>
        where
            Fi: SendImmutableQuery,
        {
            /// Serialize the world with the given alkahest serializer.
            pub fn dump_alkahest<$($f),+>(self, output: &mut Vec<u8>) -> (usize, usize)
            where
                $($f: Formula,)+
                $($c: Sync + 'static, for<'b> &'b $c: Serialize<$f>,)+
            {
                serialize_to_vec::<WorldFormula<($($f,)+)>, _>(self, output)
            }
        }

        impl<Fi $(, $f)+ $(, $c)+> Serialize<WorldFormula<($($f,)+)>> for WorldDump<'_, ($($c,)+), Fi>
        where
            Fi: SendImmutableQuery,
            $($f: Formula,)+
            $($c: Sync + 'static, for<'a> &'a $c: Serialize<$f>,)+
        {
            fn serialize<Bu>(self, sizes: &mut Sizes, mut buffer: Bu) -> Result<(), Bu::Error>
            where
                Self: Sized,
                Bu: Buffer,
            {
                let mut writer = slice_writer::<([u64; 3], DumpFormula<($($f,)+)>), _>(sizes, &mut buffer);
                <($($c,)+)>::dump_world(self.world, self.filter, self.epoch, &mut writer)?;
                writer.finish()
            }

            fn size_hint(&self) -> Option<Sizes> {
                None
            }
        }

        #[allow(non_snake_case)]
        impl<'de $(, $f)+ $(, $c)+> Deserialize<'de, DumpFormula<($($f,)+)>> for ($(LoadSlot<'_, $c>,)+)
        where
            $($f: Formula,)+
            $($c: Deserialize<'de, $f>,)+
        {
            fn deserialize(_: Deserializer<'de>) -> Result<Self, DeserializeError> {
                unreachable!("This method should not be called")
            }

            fn deserialize_in_place(
                &mut self,
                mut de: Deserializer<'de>,
            ) -> Result<(), DeserializeError>
            {
                let ($($c,)+) = self;
                $(
                    match $c {
                        LoadSlot::Skipped => {}
                        LoadSlot::Missing => {
                            let comp: $c = de.read_value::<$f, _>(false)?;
                            *$c = LoadSlot::Created(comp);
                        }
                        LoadSlot::Existing(comp) => {
                            de.read_in_place::<$f, _>(*comp, false)?;
                        }
                        LoadSlot::Created(_) => unreachable!(),
                    }
                )+
                Ok(())
            }
        }

        impl<'de $(, $f)+ $(, $c)+> Loader<($($c,)+)> for LoaderAlkahest<'de, ($($f,)+)>
        where
            $($f: Formula,)+
            $($c: Deserialize<'de, $f> + Component + Send + 'static,)+
        {
            type Error = DeserializeError;

            fn next(&mut self) -> Result<Option<EntityDump>, DeserializeError> {
                match self.iter.next() {
                    Some(Ok((entity, lazy))) => {
                        self.next = Some(lazy);
                        Ok(Some(EntityDump(entity)))
                    }
                    Some(Err(e)) => Err(e),
                    None => Ok(None),
                }
            }

            fn load(&mut self, slots: &mut ($(LoadSlot<'_, $c>,)+)) -> Result<(), DeserializeError> {
                let lazy = self.next.take().unwrap();
                lazy.get_in_place(slots)
            }
        }

        impl<Ma $(, $c)+> WorldLoad<'_, ($($c,)+), Ma>
        where
            Ma: Mark,
            $($c: Component + Send + 'static,)+
        {
            /// Deserialize the world with the given alkahest deserializer.
            pub fn load_alkahest_lazy<'de $(, $f)+>(
                &self,
                actions: &mut ActionEncoder,
                lazy: Lazy<'de, WorldFormula<($($f,)+)>>,
            ) -> Result<(), DeserializeError>
            where
                $($f: Formula,)+
                $($c: Deserialize<'de, $f>,)+
            {
                let iter = lazy.iter::<([u64; 3], Lazy<'de, DumpFormula<($($f,)+)>>)>();

                <($($c,)+)>::load_world(self.world, self.marker, actions, &mut LoaderAlkahest {
                    iter,
                    next: None,
                })
            }

            /// Deserialize the world with the given alkahest deserializer.
            pub fn load_alkahest<'de $(, $f)+>(
                &self,
                actions: &mut ActionEncoder,
                buffer: &'de [u8],
                root: usize,
            ) -> Result<(), DeserializeError>
            where
                $($f: Formula,)+
                $($c: Deserialize<'de, $f>,)+
            {
                let lazy = deserialize_with_size::<WorldFormula<($($f,)+)>, _>(buffer, root)?;
                self.load_alkahest_lazy(actions, lazy)
            }
        }
    };
}

// dumper!(A, B);
for_tuple_2!(dumper);

#[test]
fn test_dump() {
    use ::alkahest_proc::{Deserialize, Formula, SerializeRef};
    use ::edict_proc::Component;

    use super::NoMark;
    use crate::{action::ActionBuffer, epoch::EpochId, world::World};

    let mut world = World::new();

    #[derive(Component, Debug, PartialEq, Eq, Formula, SerializeRef, Deserialize)]
    struct Foo;

    #[derive(Component, Debug, PartialEq, Eq, Formula, SerializeRef, Deserialize)]
    struct Bar(u32);

    #[derive(Component, Debug, PartialEq, Eq, Formula, SerializeRef, Deserialize)]
    struct Baz(String);

    let foo = world.spawn((Foo,)).id();
    let bar = world.spawn((Bar(42),)).id();
    let baz = world.spawn((Baz("qwerty".into()),)).id();

    let foo_bar = world.spawn((Foo, Bar(11))).id();
    let foo_baz = world.spawn((Foo, Baz("asdfgh".into()))).id();
    let bar_baz = world.spawn((Bar(23), Baz("zxcvbn".into()))).id();
    let foo_bar_baz = world.spawn((Foo, Bar(155), Baz("123456".into()))).id();

    type Set = (Foo, Bar, Baz);

    let mut data = Vec::new();
    let (size, root) = WorldDump::<Set, _>::new(&mut world, (), EpochId::start())
        .dump_alkahest::<Foo, Bar, Baz>(&mut data);

    let mut world2 = World::new();

    let mut buffer = ActionBuffer::new();
    let mut actions = buffer.encoder(&world2);

    WorldLoad::<Set, _>::new(&world2, NoMark)
        .load_alkahest::<Foo, Bar, Baz>(&mut actions, &data[..size], root)
        .unwrap();
    buffer.execute(&mut world2);

    assert_eq!(
        world2.get::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(foo),
        Ok((Some(&Foo), None, None))
    );

    assert_eq!(
        world2.get::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(bar),
        Ok((None, Some(&Bar(42)), None))
    );

    assert_eq!(
        world2.get::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(baz),
        Ok((None, None, Some(&Baz("qwerty".into()))))
    );

    assert_eq!(
        world2.get::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(foo_bar),
        Ok((Some(&Foo), Some(&Bar(11)), None))
    );

    assert_eq!(
        world2.get::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(foo_baz),
        Ok((Some(&Foo), None, Some(&Baz("asdfgh".into()))))
    );

    assert_eq!(
        world2.get::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(bar_baz),
        Ok((None, Some(&Bar(23)), Some(&Baz("zxcvbn".into()))))
    );

    assert_eq!(
        world2.get::<(Option<&Foo>, Option<&Bar>, Option<&Baz>)>(foo_bar_baz),
        Ok((Some(&Foo), Some(&Bar(155)), Some(&Baz("123456".into()))))
    );
}
