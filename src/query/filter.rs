use core::{any::TypeId, marker::PhantomData};

use crate::{archetype::Archetype, component::Component};

/// Filters for query iterators.
/// They affect what archetypes and entities are skipped by query iterator
/// but do not affect iterator item.
pub trait Filter {
    /// Checks if filter requires archetype to be skipped.
    #[inline]
    fn skip_archetype(&self, archetype: &Archetype, tracks: u64, epoch: u64) -> bool {
        drop(archetype);
        drop(tracks);
        drop(epoch);
        false
    }
}

/// Filter that allows only archetypes with specified component.
#[derive(Clone, Copy, Debug, Default)]
pub struct With<T> {
    marker: PhantomData<T>,
}

impl<T> With<T> {
    /// Returns new instance of `With` filter.
    pub const fn new() -> Self {
        With {
            marker: PhantomData,
        }
    }
}

impl<T> Filter for With<T>
where
    T: Component,
{
    #[inline]
    fn skip_archetype(&self, archetype: &Archetype, tracks: u64, epoch: u64) -> bool {
        drop(tracks);
        drop(epoch);
        !archetype.contains_id(TypeId::of::<T>())
    }
}

/// Filter that allows only archetypes without specified component.
#[derive(Clone, Copy, Debug, Default)]
pub struct Without<T> {
    marker: PhantomData<T>,
}

impl<T> Without<T> {
    /// Returns new instance of `Without` filter.
    pub const fn new() -> Self {
        Without {
            marker: PhantomData,
        }
    }
}

impl<T> Filter for Without<T>
where
    T: Component,
{
    #[inline]
    fn skip_archetype(&self, archetype: &Archetype, tracks: u64, epoch: u64) -> bool {
        drop(tracks);
        drop(epoch);
        archetype.contains_id(TypeId::of::<T>())
    }
}
