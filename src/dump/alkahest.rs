//! Serialization support for the entire [`World`](crate::world::World).
//!
//! This module provides the [`ComponentDump`](ComponentDump) trait, which can be implemented for

use alkahest::{Formula, Serialize, Serializer, SliceWriter};

use crate::{epoch::EpochId, world::World};

use super::{DumpSet, DumpSlot, Dumper, EntityDump};

struct DumpFormula<F, T>(F, T);

impl<F, T> Formula for DumpFormula<F, T>
where
    F: Formula,
{
    const EXACT_SIZE: bool = false;
    const HEAPLESS: bool = false;
    const MAX_STACK_SIZE: Option<usize> = None;
}

struct SerializeDump<T>([u64; 3], T);

/// Formula for serializing world with set of components.
pub struct WorldFormula<F, T = F>(F, T);

impl<T, F> Formula for WorldFormula<F, T>
where
    T: DumpSet,
{
    const MAX_STACK_SIZE: Option<usize> = None;
    const EXACT_SIZE: bool = false;
    const HEAPLESS: bool = false;
}

macro_rules! dumper {
    (,) => {};
    ($($c:ident)+,$($f:ident)+) => {
        #[allow(non_snake_case)]
        impl<'a $(, $f)+ $(, $c)+> Serialize<DumpFormula<($($f,)+), ($($c,)+)>> for SerializeDump<($(DumpSlot<'a, $c>,)+)>
        where
            $($f: Formula, &'a $c: Serialize<$f>,)+
        {
            fn serialize<Se>(self, serializer: impl Into<Se>) -> Result<Se::Ok, Se::Error>
            where
                Se: Serializer,
            {
                let mut ser = serializer.into();
                ser.write_value::<[u64; 3], _>(self.0)?;
                let ($($c,)+) = self.1;
                $(
                    match $c {
                        DumpSlot::Skipped => {}
                        DumpSlot::Component($c) => ser.write_value::<$f, &$c>($c)?,
                    }
                )+
                ser.finish()
            }

            fn size_hint(&self) -> Option<usize> {
                None
            }
        }

        impl<'a, Se $(, $f)+ $(, $c)+> Dumper<($($c,)+)> for SliceWriter<'a, DumpFormula<($($f,)+), ($($c,)+)>, Se>
        where
            Se: Serializer + ?Sized,
            $($f: Formula, $c: Sync + 'static, for<'b> &'b $c: Serialize<$f>,)+
        {
            type Error = Se::Error;
            fn dump(&mut self, entity: EntityDump, tuple: ($(DumpSlot<'_, $c>,)+)) -> Result<(), Se::Error> {
                self.write_elem(SerializeDump(entity.0, tuple))
            }
        }

        impl<$($f,)+ $($c),+> Serialize<WorldFormula<($($f,)+), ($($c,)+)>> for &World
        where
            $($f: Formula, $c: Sync + 'static, for<'a> &'a $c: Serialize<$f>,)+
        {
            fn serialize<S>(self, serializer: impl Into<S>) -> Result<S::Ok, S::Error>
            where
                Self: Sized,
                S: Serializer,
            {
                let mut ser = serializer.into();
                let mut writer = ser.slice_writer::<DumpFormula<($($f,)+), ($($c,)+)>>();
                <($($c,)+)>::dump_world(self, EpochId::start(), &mut writer)?;
                writer.finish()?;
                ser.finish()
            }

            fn size_hint(&self) -> Option<usize> {
                None
            }
        }
    };
}

for_tuple_2!(dumper);
