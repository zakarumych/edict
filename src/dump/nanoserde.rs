//! Serialization support for the entire [`World`](crate::world::World).
//!
//! This module provides the [`ComponentDump`](ComponentDump) trait, which can be implemented for

use core::convert::Infallible;

use nanoserde::SerBin;

use super::{DumpSlot, Dumper, EntityDump};

macro_rules! dumper {
    () => {};
    ($($a:ident)+) => {
        #[allow(non_snake_case)]
        impl<'a $(, $a)+> Dumper<($($a,)+)> for &'a mut Vec<u8>
        where
            $($a: SerBin + Sync + 'static,)+
        {
            type Error = Infallible;
            fn dump(&mut self, entity: EntityDump, slots: ($(DumpSlot<'_, $a>,)+)) -> Result<(), Infallible> {
                entity.0.ser_bin(self);
                let ($($a,)+) = slots;
                $(
                    match $a {
                        DumpSlot::Skipped => {}
                        DumpSlot::Component($a) => $a.ser_bin(self),
                    }
                )+
                Ok(())
            }
        }
    };
}

for_tuple!(dumper);
