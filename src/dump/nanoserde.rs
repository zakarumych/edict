//! Serialization support for the entire [`World`](crate::world::World).
//!
//! This module provides the [`ComponentDump`](ComponentDump) trait, which can be implemented for

use core::convert::Infallible;

use nanoserde::{SerBin, SerJson, SerJsonState, SerRon, SerRonState};

/// Dumps world using [`nanoserde::SerBin`].
pub struct DumperBin<'a>(pub &'a mut Vec<u8>);

/// Dumps world using [`nanoserde::SerJson`].
pub struct DumperJson<'a>(pub &'a mut SerJsonState);

/// Dumps world using [`nanoserde::SerRon`].
pub struct DumperRon<'a>(pub &'a mut SerRonState);

use super::{DumpSlot, Dumper, EntityDump};

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

        #[allow(non_snake_case)]
        impl<'a $(, $a)+> Dumper<($($a,)+)> for DumperJson<'a>
        where
            $($a: SerJson + Sync + 'static,)+
        {
            type Error = Infallible;
            fn dump(&mut self, entity: EntityDump, slots: ($(DumpSlot<'_, $a>,)+)) -> Result<(), Infallible> {
                entity.0.ser_json(0, self.0);
                let ($($a,)+) = slots;
                $(
                    match $a {
                        DumpSlot::Skipped => {}
                        DumpSlot::Component($a) => $a.ser_json(0, self.0),
                    }
                )+
                Ok(())
            }
        }

        #[allow(non_snake_case)]
        impl<'a $(, $a)+> Dumper<($($a,)+)> for DumperRon<'a>
        where
            $($a: SerRon + Sync + 'static,)+
        {
            type Error = Infallible;
            fn dump(&mut self, entity: EntityDump, slots: ($(DumpSlot<'_, $a>,)+)) -> Result<(), Infallible> {
                entity.0.ser_ron(0, self.0);
                let ($($a,)+) = slots;
                $(
                    match $a {
                        DumpSlot::Skipped => {}
                        DumpSlot::Component($a) => $a.ser_ron(0, self.0),
                    }
                )+
                Ok(())
            }
        }
    };
}

for_tuple!(dumper);
