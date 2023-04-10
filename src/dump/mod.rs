//! Provides world serialization integration with serialization crates.
//!
//! Supports
//! - `serde`
//! - `nanoserde`
//! - `alkahest`
//!
//! Each can be enabled with a feature named as serialization crate.

mod query;

use core::marker::PhantomData;

use crate::{
    component::Component,
    entity::EntityId,
    epoch::EpochId,
    query::ImmutableQuery,
    world::{QueryOneError, World},
    ActionEncoder,
};

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
pub trait Loader<T: LoadSet + ?Sized> {
    /// Type of possible errors that can occur during deserialization.
    type Error;

    /// Returns next entity to load.
    fn next(&mut self) -> Result<Option<EntityDump>, Self::Error>;

    /// Loads entity with provided component slots.
    fn load(&mut self, slots: &mut T::LoadSlots<'_>) -> Result<(), Self::Error>;
}

/// Marker for loaded entities.
pub trait Mark: Copy + Send + 'static {
    /// Marks loaded entity.
    /// This method can add a component or relation to the entity.
    fn mark(&self, world: &mut World, id: EntityId);
}

impl<F> Mark for F
where
    F: Fn(&mut World, EntityId) + Copy + Send + 'static,
{
    #[inline]
    fn mark(&self, world: &mut World, id: EntityId) {
        self(world, id)
    }
}

/// Leaves no mark or loaded entities.
#[derive(Clone, Copy)]
pub struct NoMark;

impl Mark for NoMark {
    #[inline]
    fn mark(&self, _: &mut World, _: EntityId) {}
}

/// Tuple of components that can be dumped.
pub trait DumpSet {
    /// Tuple of dump slots of component types.
    type DumpSlots<'a>;

    /// Serializes entities from the world.
    /// Calls `dumper` for each entity
    /// with opaque integers and tuple of slots.
    /// The `dumper` should perform serialization of those values.
    /// The loading process will run in reverse, where loader will have to
    /// deserialize integer triples and tuple of slots
    /// and feed into loading function.
    fn dump_world<F, D, E>(
        world: &World,
        filter: F,
        after_epoch: EpochId,
        dumper: &mut D,
    ) -> Result<(), E>
    where
        F: ImmutableQuery,
        D: for<'a> Dumper<Self, Error = E>;
}

/// Tuple of components that can be loaded.
pub trait LoadSet {
    /// Tuple of load slots of component types.
    type LoadSlots<'a>;

    /// Loads serialized entities into the world.
    fn load_world<L, E, M>(
        world: &World,
        marker: M,
        actions: &mut ActionEncoder,
        loader: &mut L,
    ) -> Result<(), E>
    where
        L: for<'a> Loader<Self, Error = E>,
        M: Mark;
}

/// Wrapper for `World` that implements `Serialize`.
pub struct WorldDump<'a, T, F> {
    /// World to dump from.
    pub world: &'a World,

    /// Filter to filter entities.
    pub filter: F,

    /// Epoch to dump after.
    pub epoch: EpochId,
    marker: PhantomData<fn() -> T>,
}

impl<'a, T, F> WorldDump<'a, T, F> {
    /// Creates new `WorldDump` with provided world and filter.
    pub fn new(world: &'a World, filter: F, epoch: EpochId) -> Self {
        WorldDump {
            world,
            filter,
            epoch,
            marker: PhantomData,
        }
    }
}

/// Wrapper for `World` that implements `Serialize`.
pub struct WorldLoad<'a, T, M> {
    /// World to load into.
    pub world: &'a World,

    /// Marker to mark loaded entities.
    pub marker: M,
    _marker: PhantomData<fn() -> T>,
}

impl<'a, T, M> WorldLoad<'a, T, M> {
    /// Creates new `WorldLoad` with provided world and marker.
    pub fn new(world: &'a World, marker: M) -> Self {
        WorldLoad {
            world,
            marker,
            _marker: PhantomData,
        }
    }
}

macro_rules! set {
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

            #[inline]
            fn dump_world<Fi, Du, Er>(world: &World, filter: Fi, after_epoch: EpochId, dumper: &mut Du) -> Result<(), Er>
            where
                Fi: ImmutableQuery,
                Du: for<'a> Dumper<($($a,)+), Error = Er>,
            {
                let mut query = world.query_with(DumpQuery::<($($a,)+)>::new(after_epoch)).filter(filter);

                query.try_for_each(|(id, ($($a),+))| {
                    let mut present = 0;
                    let mut modified = 0;

                    let slots = indexed_tuple!(idx => $(match $a {
                        DumpItem::Missing => DumpSlot::Skipped,
                        DumpItem::Modified(comp) => {
                            modified |= (1 << idx);
                            DumpSlot::Component(comp)
                        }
                        DumpItem::Unmodified => {
                            present |= (1 << idx);
                            DumpSlot::Skipped
                        }
                    }),+);
                    let bits = id.bits();
                    dumper.dump(EntityDump([bits, present | modified, modified]), slots)
                })
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_assignments)]
        #[allow(unused_parens)]
        impl<$($a),+> LoadSet for ($($a,)+)
        where
            $($a: Component + Send + 'static,)+
        {
            type LoadSlots<'a> = ($(LoadSlot<'a, $a>,)+);

            fn load_world<Lo, Er, Ma>(
                world: &World,
                marker: Ma,
                actions: &mut ActionEncoder,
                loader: &mut Lo,
            ) -> Result<(), Er>
            where
                Lo: for<'a> Loader<($($a,)+), Error = Er>,
                Ma: Mark,
            {
                let mut query = world.query::<($(Option<&mut $a>,)+)>();

                while let Some(next) = loader.next()? {
                    let EntityDump([bits, present, modified]) = next;
                    let Some(id) = EntityId::from_bits(bits) else {
                        continue;
                    };

                    let mut slots = match query.get_one(id) {
                        Ok(($($a),+)) => {
                            indexed_tuple!(idx => $(
                                if modified & (1 << idx) == 0 {
                                    LoadSlot::Skipped
                                } else {
                                    match $a {
                                        Some(comp) => LoadSlot::Existing(comp),
                                        None => LoadSlot::Missing,
                                    }
                                }
                            ),+)
                        }
                        Err(QueryOneError::NotSatisfied) => unreachable!("Tuple of options is always satisfied"),
                        Err(QueryOneError::NoSuchEntity) => {
                            indexed_tuple!(idx => $(
                                if modified & (1 << idx) != 0 {
                                    LoadSlot::<$a>::Missing
                                } else {
                                    LoadSlot::<$a>::Skipped
                                }
                            ),+)
                        }
                    };

                    loader.load(&mut slots)?;

                    let ($($a,)+) = slots;
                    $(
                        let $a = match $a {
                            LoadSlot::Skipped => None,
                            LoadSlot::Missing => {
                                unreachable!("Must be created by loader");
                            }
                            LoadSlot::Existing(_) => None,
                            LoadSlot::Created(comp) => Some(comp),
                        };
                    )+

                    let marker = marker.clone();
                    actions.closure(move |world| {
                        world.spawn_if_missing(id);
                        marker.mark(world, id);
                        indexed_tuple!(idx => $(match $a {
                            Some(comp) => { let _ = world.insert(id, comp); }
                            None => if present & (1 << idx) != 0 {
                                let _ = world.drop::<$a>(id);
                            }
                        }),+);
                    });
                }

                Ok(())
            }
        }
    };
}

for_tuple!(set);
