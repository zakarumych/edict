//! Serialization support for the entire [`World`](crate::world::World).
//!
//! This module provides the [`ComponentDump`](ComponentDump) trait, which can be implemented for

use serde::ser::{Serialize, SerializeSeq, Serializer};

use crate::query::ImmutableQuery;

use super::{DumpSet, DumpSlot, Dumper, EntityDump, WorldDump};

/// Wrapper for `serde::ser::SerializeSeq` that implements `Dumper`.
pub struct SerdeDumper<'a, S>(pub &'a mut S);

struct SerializeDump<T>([u64; 3], T);

macro_rules! dumper {
    () => {};
    ($($a:ident)+) => {
        #[allow(non_snake_case)]
        impl<'a $(, $a)+> Serialize for SerializeDump<($(DumpSlot<'a, $a>,)+)>
        where
            $($a: Serialize,)+
        {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut seq = serializer.serialize_seq(None)?;
                seq.serialize_element(&self.0)?;
                let ($($a,)+) = &self.1;
                $(
                    match $a {
                        DumpSlot::Skipped => {}
                        DumpSlot::Component($a) => seq.serialize_element($a)?,
                    }
                )+
                seq.end()
            }
        }

        impl<'a $(, $a)+, Se> Dumper<($($a,)+)> for SerdeDumper<'_, Se>
        where
            $($a: Serialize + Sync + 'static,)+
            Se: SerializeSeq,
        {
            type Error = Se::Error;
            fn dump(&mut self, entity: EntityDump, slots: ($(DumpSlot<'_, $a>,)+)) -> Result<(), Se::Error> {
                self.0.serialize_element(&SerializeDump(entity.0, slots))
            }
        }

        impl<'a $(, $a)+, Fi> Serialize for WorldDump<'a, ($($a,)+), Fi>
        where
            Fi: ImmutableQuery + Copy,
            $($a: Serialize + Sync + 'static,)+
        {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut seq = serializer.serialize_seq(None)?;
                let mut dumper = SerdeDumper(&mut seq);
                <($($a,)+) as DumpSet>::dump_world(self.world, self.filter, self.epoch, &mut dumper)?;
                seq.end()
            }
        }
    };
}

for_tuple!(dumper);
