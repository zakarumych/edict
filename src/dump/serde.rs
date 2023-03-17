//! Serialization support for the entire [`World`](crate::world::World).
//!
//! This module provides the [`ComponentDump`](ComponentDump) trait, which can be implemented for

use core::marker::PhantomData;

use serde::ser::{Serialize, SerializeSeq, SerializeTuple, Serializer};

use crate::{
    epoch::EpochId,
    query::{Modified, Query, QueryItem},
    world::World,
};

use super::DumpId;

/// Implements dumping for a component.
pub trait ComponentDump {
    /// Serializable component dump.
    type Dump<'a>: Serialize
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
    fn dump_item<S>(
        id: DumpId<X>,
        item: &QueryItem<'_, Self::ItemQuery>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer;

    /// Serializes the entire world.
    fn serialize_world<S>(epoch: EpochId, world: &World, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut q = world
            .query::<&DumpId<X>>()
            .extend_query(Self::query_item(epoch));

        let mut seq = serializer.serialize_seq(None)?;

        q.iter_mut()
            .try_for_each(|(id, item)| seq.serialize_element(&DumpItem::<X, Self>(*id, item)))?;

        seq.end()
    }
}

struct DumpItem<'a, X: 'static, D: Dump<X> + ?Sized>(DumpId<X>, QueryItem<'a, D::ItemQuery>);

impl<X, D> Serialize for DumpItem<'_, X, D>
where
    D: Dump<X> + ?Sized,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        Self: Sized,
        S: Serializer,
    {
        D::dump_item(self.0, &self.1, serializer)
    }
}

struct DumpElem<'a, T: ?Sized>(&'a T);

impl<T> Serialize for DumpElem<'_, T>
where
    T: ComponentDump + ?Sized,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        Self: Sized,
        S: Serializer,
    {
        self.0.dump().serialize(serializer)
    }
}

macro_rules! count {
    () => { 0 };
    ($a:tt $($b:tt)*) => {
        1 + count!($($b)*)
    };
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
            fn dump_item<S>(
                id: DumpId<X>,
                tuple: &($(Option<&$a>),*),
                serializer: S,
            ) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let ($($a),*) = tuple;
                let mut tuple = serializer.serialize_tuple(count!($($a)*) + 1)?;
                tuple.serialize_element(&id.0)?;
                $(tuple.serialize_element(&$a.map(DumpElem))?;)*
                tuple.end()
            }
        }
    };
}

for_tuple!(impl_component_dump);

/// Formula for dumping the entire world
/// using `X` marker and `T` set of components.
pub struct WorldFormula<'a, X, T>(&'a World, PhantomData<fn() -> (X, T)>);

impl<X, T> Serialize for WorldFormula<'_, X, T>
where
    X: 'static,
    T: Dump<X>,
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        T::serialize_world(EpochId::start(), self.0, serializer)
    }
}
