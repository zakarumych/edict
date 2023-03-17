//! Serialization support for the entire [`World`](crate::world::World).
//!
//! This module provides the [`ComponentDump`](ComponentDump) trait, which can be implemented for

use core::marker::PhantomData;

use alkahest::{Formula, Serialize, Serializer};

use crate::{
    epoch::EpochId,
    query::{Modified, Query, QueryItem},
    world::World,
};

use super::DumpId;

/// Implements dumping for a component.
pub trait ComponentDump {
    /// Formula used to serialize the component.
    type Formula: Formula;

    /// Serializable component dump.
    type Dump<'a>: Serialize<Self::Formula>
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
    /// Formula used to serialize the component.
    type ItemFormula: Formula;

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
        item: QueryItem<'_, Self::ItemQuery>,
        serializer: impl Into<S>,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer;

    /// Returns size hint for the item.
    fn size_hint_item(item: &QueryItem<'_, Self::ItemQuery>) -> Option<usize>;

    /// Serializes the entire world.
    fn serialize_world<S>(
        epoch: EpochId,
        world: &World,
        serializer: impl Into<S>,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut q = world
            .query::<&DumpId<X>>()
            .extend_query(Self::query_item(epoch));

        let mut ser = serializer.into();
        ser.write_slice::<(u64, Self::ItemFormula), _>(
            q.iter_mut()
                .map(|(id, item)| DumpItem::<X, Self>(*id, item)),
        )?;
        ser.finish()
    }
}

struct DumpItem<'a, X: 'static, D: Dump<X> + ?Sized>(DumpId<X>, QueryItem<'a, D::ItemQuery>);

impl<F, X, D> Serialize<(u64, F)> for DumpItem<'_, X, D>
where
    F: Formula + ?Sized,
    X: 'static,
    D: Dump<X> + ?Sized,
{
    fn serialize<S>(self, serializer: impl Into<S>) -> Result<S::Ok, S::Error>
    where
        Self: Sized,
        S: Serializer,
    {
        D::dump_item(self.0, self.1, serializer)
    }

    fn size_hint(&self) -> Option<usize> {
        let id_size = <u64 as Serialize<u64>>::size_hint(&self.0 .0)?;
        let item_size = D::size_hint_item(&self.1)?;
        id_size.checked_add(item_size)
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
            type ItemFormula = ($($a::Formula,)*);
            type ItemQuery = ($(Modified<Option<&'static $a>>,)*);

            #[inline]
            fn query_item(epoch: EpochId) -> Self::ItemQuery {
                ($(
                    Modified::<Option<&'static $a>>::new(epoch),
                )*)
            }

            #[inline]
            fn size_hint_item(tuple: &($(Option<&$a>),*)) -> Option<usize> {
                let ($($a),*) = tuple;
                let mut size = 0;
                $(if let Some($a) = $a {
                    size += $a.dump().size_hint()?;
                })*
                Some(size)
            }

            #[inline]
            fn dump_item<S>(
                id: DumpId<X>,
                tuple: ($(Option<&$a>),*),
                ser: impl Into<S>,
            ) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut ser = ser.into();
                ser.write_value::<u64, _>(id.0)?;

                let ($($a),*) = tuple;

                $(match $a {
                    None => ser.write_value::<u8, _>(0)?,
                    Some($a) => {
                        ser.write_value::<u8, _>(1)?;
                        ser.write_value::<$a::Formula, _>($a.dump())?;
                    }
                })*

                ser.finish()
            }
        }
    };
}

impl_component_dump!(A);
impl_component_dump!(A B);
impl_component_dump!(A B C);
impl_component_dump!(A B C D);
impl_component_dump!(A B C D E);
impl_component_dump!(A B C D E F);
impl_component_dump!(A B C D E F G);
impl_component_dump!(A B C D E F G H);

/// Formula for dumping the entire world
/// using `X` marker and `T` set of components.
pub struct WorldFormula<X, T>(T, PhantomData<fn() -> X>);

impl<X, T> Formula for WorldFormula<X, T>
where
    X: 'static,
    T: Dump<X>,
{
    const EXACT_SIZE: bool = false;
    const HEAPLESS: bool = false;
    const MAX_STACK_SIZE: Option<usize> = None;
}

impl<X, T> Serialize<WorldFormula<X, T>> for &World
where
    X: 'static,
    T: Dump<X>,
{
    #[inline]
    fn serialize<S>(self, serializer: impl Into<S>) -> Result<S::Ok, S::Error>
    where
        Self: Sized,
        S: Serializer,
    {
        T::serialize_world(EpochId::start(), self, serializer)
    }

    #[inline]
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
