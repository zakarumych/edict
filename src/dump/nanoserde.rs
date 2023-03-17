//! Serialization support for the entire [`World`](crate::world::World).
//!
//! This module provides the [`ComponentDump`](ComponentDump) trait, which can be implemented for

use core::marker::PhantomData;

use nanoserde::SerBin;

use crate::{
    epoch::EpochId,
    query::{Modified, Query, QueryItem},
    world::World,
};

use super::DumpId;

/// Implements dumping for a component.
pub trait ComponentDump {
    /// Serializable component dump.
    type Dump<'a>: SerBin
    where
        Self: 'a;

    /// Returns the component dump from shared reference.
    fn dump(&self) -> Self::Dump<'_>;
}

/// Trait for a set of dumpable components.
/// This trait is implemented for tuples of [`ComponentDump`]s up to 12 elements.
///
/// It can be derived for a structure that contains any number of fields
/// where all fields implement [`ComponentDump`].
pub trait Dump<X: 'static> {
    /// Query used to get the dumpable components.
    type ItemQuery: Query;

    /// Returns the query for the dump.
    ///
    /// Epoch is used to get only components that were modified after the epoch.
    /// Use [`EpochId::start()`](EpochId::start) to get all components.
    fn query_item(epoch: EpochId) -> Self::ItemQuery;

    /// Dumps a single item from the query.
    fn dump_item(id: DumpId<X>, item: QueryItem<'_, Self::ItemQuery>, output: &mut Vec<u8>);

    /// Serializes the entire world.
    fn serialize_world(epoch: EpochId, world: &World, output: &mut Vec<u8>) {
        let mut q = world
            .query::<&DumpId<X>>()
            .extend_query(Self::query_item(epoch));

        q.iter_mut()
            .for_each(|(id, item)| Self::dump_item(*id, item, output))
    }
}

macro_rules! impl_component_dump {
    ($($a:ident)*) => {
        #[allow(unused_parens, non_snake_case)]
        impl<X $(, $a)*> Dump<X> for ($($a,)*)
        where
            X: 'static,
            $($a : ComponentDump + Sync + 'static,)*
        {
            type ItemQuery = ($(Modified<Option<&'static $a>>,)*);

            #[inline]
            #[allow(unused_variables)]
            fn query_item(epoch: EpochId) -> Self::ItemQuery {
                ($(
                    Modified::<Option<&'static $a>>::new(epoch),
                )*)
            }

            #[inline]
            fn dump_item(
                id: DumpId<X>,
                tuple: ($(Option<&$a>),*),
                output: &mut Vec<u8>,
            ) {
                id.0.ser_bin(output);
                let ($($a),*) = tuple;
                $(match $a {
                    None => output.push(0),
                    Some($a) => {
                        output.push(1);
                        $a.dump().ser_bin(output);
                    }
                })*
            }
        }
    };
}

for_tuple!(impl_component_dump);

/// Formula for dumping the entire world
/// using `X` marker and `T` set of components.
pub struct WorldFormula<'a, X, T>(&'a World, PhantomData<fn() -> (X, T)>);

impl<X, T> SerBin for WorldFormula<'_, X, T>
where
    X: 'static,
    T: Dump<X>,
{
    #[inline]
    fn ser_bin(&self, output: &mut Vec<u8>) {
        T::serialize_world(EpochId::start(), self.0, output);
    }
}
