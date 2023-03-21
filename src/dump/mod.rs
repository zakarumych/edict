//! Provides world serialization integration with serialization crates.
//!
//! Supports
//! - `serde`
//! - `nanoserde`
//! - `alkahest`
//!
//! Each can be enabled with a feature named as serialization crate.

mod query;

use crate::{epoch::EpochId, world::World};

pub use self::query::DumpItem;
use self::query::DumpQuery;

#[cfg(feature = "alkahest")]
pub mod alkahest;

#[cfg(feature = "nanoserde")]
pub mod nanoserde;

#[cfg(feature = "serde")]
pub mod serde;

/// Opaque integer triple that needs to be serialized and deserialized.
pub struct EntityDump(pub [u64; 3]);

/// Serializer implementation.
pub trait Dumper<T: DumpSet + ?Sized> {
    /// Type of possible errors that can occur during serialization.
    type Error;

    /// Serialize entity with provided component slots.
    fn dump(&mut self, entity: EntityDump, tuple: T::DumpSlots<'_>) -> Result<(), Self::Error>;
}

/// Slot of component value to be serialized.
pub enum DumpSlot<'a, T> {
    /// Skip this slot.
    Skipped,

    /// Serialize component value.
    Component(&'a T),
}

/// Slot of component value to be deserialized.
pub enum LoadSlot<'a, T> {
    /// Do not deserialize this slot.
    Skipped,

    /// Deserialize into new component value.
    Missing,

    /// Deserialize into existing component value.
    Existing(&'a mut T),

    /// Newly created component.
    Created(T),
}

/// Deserializer implementation.
pub trait Loader<T: DumpSet + ?Sized> {
    /// Type of possible errors that can occur during deserialization.
    type Error;

    /// Returns next entity to load.
    fn next(&mut self) -> Result<Option<EntityDump>, Self::Error>;

    /// Loads entity with provided component slots.
    fn load(&mut self, slots: &mut T::LoadSlots<'_>) -> Result<(), Self::Error>;
}

/// Tuple of components that can be dumped.
pub trait DumpSet {
    /// Tuple of dump slots of component types.
    type DumpSlots<'a>;

    /// Tuple of load slots of component types.
    type LoadSlots<'a>;

    /// Serializes entities from the world.
    /// Calls `dumper` for each entity
    /// with opaque integers and tuple of slots.
    /// The `dumper` should perform serialization of those values.
    /// The loading process will run in reverse, where loader will have to
    /// deserialize integer triples and tuple of slots
    /// and feed into loading function.
    fn dump_world<D, E>(world: &World, after_epoch: EpochId, dumper: &mut D) -> Result<(), E>
    where
        D: for<'a> Dumper<Self, Error = E>;

    /// Loads serialized entities into the world.
    fn load_world<L, E>(world: &mut World, loader: &mut L) -> Result<(), E>
    where
        L: for<'a> Loader<Self, Error = E>;
}

macro_rules! dump_set {
    () => {
        /* Don't implement for empty tuple */
    };
    ($($a:ident)+) => {
        #[allow(non_snake_case)]
        #[allow(unused_assignments)]
        #[allow(unused_parens)]
        impl<$($a),+> DumpSet for ($($a,)+)
        where
            $($a: Sync + 'static,)+
        {
            type DumpSlots<'a> = ($(DumpSlot<'a, $a>,)+);
            type LoadSlots<'a> = ($(LoadSlot<'a, $a>,)+);

            #[inline]
            fn dump_world<Du, Er>(world: &World, after_epoch: EpochId, dumper: &mut Du) -> Result<(), Er>
            where
                Du: for<'a> Dumper<($($a,)+), Error = Er>,
            {
                let mut query = world.query_with(DumpQuery::<($($a,)+)>::new(after_epoch));

                query.try_for_each(|(id, ($($a),+))| {
                    let mut present = 0;
                    let mut modified = 0;
                    let mut idx = 0;
                    let slots = ($({
                        let opt = match $a {
                            DumpItem::Missing => DumpSlot::Skipped,
                            DumpItem::Modified(comp) => {
                                modified |= (1 << idx);
                                DumpSlot::Component(comp)
                            }
                            DumpItem::Unmodified => {
                                present |= (1 << idx);
                                DumpSlot::Skipped
                            }
                        };
                        idx += 1;
                        opt
                    },)+);
                    let bits = id.bits();
                    dumper.dump(EntityDump([bits, present, modified]), slots)
                })
            }

            fn load_world<Lo, Er>(_world: &mut World, _loader: &mut Lo) -> Result<(), Er>
            where
                Lo: for<'a> Loader<($($a,)+), Error = Er>,
            {
                todo!()
            }
        }
    };
}

for_tuple!(dump_set);
