//! This module implements [`Component`] trait for all suitable types.

use core::{
    alloc::Layout,
    any::{type_name, TypeId},
    ptr::{self, drop_in_place, slice_from_raw_parts_mut},
};

/// Trait that is implemented for all types that can act as a component.
/// Currently is implemented for all `'static` types.
pub trait Component: 'static {
    /// Returns [`ComponentInfo`] for this component type.
    fn info() -> ComponentInfo;
}

impl<T> Component for T
where
    T: 'static,
{
    fn info() -> ComponentInfo {
        ComponentInfo::of::<T>()
    }
}

/// Type information required for components.
#[derive(Clone, Copy, Debug)]
pub struct ComponentInfo {
    /// [`TypeId`] of the component.
    pub id: TypeId,

    /// [`Layout`] of the component.
    pub layout: Layout,

    /// [`type_name`] of the component.
    pub debug_name: &'static str,

    /// Function that calls drop glue for an array components.
    pub drop: unsafe fn(*mut u8, usize),

    /// Function that calls drop glue for a component.
    pub drop_one: unsafe fn(*mut u8),

    /// Function that replaces component at target location.
    pub set_one: unsafe fn(*mut u8, *mut u8),
}

impl ComponentInfo {
    /// Returns component information for specified component type.
    pub fn of<T>() -> Self
    where
        T: Component,
    {
        ComponentInfo {
            id: TypeId::of::<T>(),
            layout: Layout::new::<T>(),
            debug_name: type_name::<T>(),
            drop: |ptr, count| unsafe {
                drop_in_place::<[T]>(slice_from_raw_parts_mut(ptr.cast::<T>(), count))
            },
            drop_one: |ptr| unsafe { drop_in_place::<T>(ptr.cast()) },
            set_one: |src, dst| unsafe { *(dst as *mut T) = ptr::read(src as *mut T) },
        }
    }
}
