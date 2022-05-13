use core::{
    any::TypeId,
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    archetype::{chunk_idx, Archetype},
    component::Component,
};

use super::{Access, Fetch, NonTrackingQuery, Query};

/// Query type that is an alternative to `&mut T`.
/// Yields mutable reference wrapper that bumps component version on dereference.
/// In contrast with `&mut T` that bumps component version on yield, but works faster.
/// Use this query if redundant version bumps would cause heavy calculations.
///
/// `Alt` is `NonTrackingQuery` as it does not depend on current versions
/// of the components.
#[derive(Clone, Copy, Debug)]
pub struct Alt<T>(PhantomData<T>);

/// Item type that `Alt` yields.
/// Wraps `&mut T` and implements `DerefMut` to `T`.
/// Bumps component version on dereference.
#[derive(Debug)]
pub struct RefMut<'a, T: ?Sized> {
    pub(super) component: &'a mut T,
    pub(super) entity_version: &'a mut u64,
    pub(super) chunk_version: &'a Cell<u64>,
    pub(super) epoch: u64,
}

impl<T> Deref for RefMut<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &*self.component
    }
}

impl<T> DerefMut for RefMut<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        *self.entity_version = self.epoch;
        self.chunk_version.set(self.epoch);
        self.component
    }
}

/// `Fetch` type for the `Alt` query.
#[allow(missing_debug_implementations)]
pub struct FetchAlt<T> {
    epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<Cell<u64>>,
}

impl<'a, T> Fetch<'a> for FetchAlt<T>
where
    T: Component,
{
    type Item = RefMut<'a, T>;

    #[inline]
    fn dangling() -> Self {
        FetchAlt {
            epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn skip_item(&self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RefMut<'a, T> {
        RefMut {
            component: &mut *self.ptr.as_ptr().add(idx),
            entity_version: &mut *self.entity_versions.as_ptr().add(idx),
            chunk_version: &*self.chunk_versions.as_ptr().add(chunk_idx(idx)),
            epoch: self.epoch,
        }
    }
}

unsafe impl<T> Query for Alt<T>
where
    T: Component,
{
    type Fetch = FetchAlt<T>;

    #[inline]
    fn mutates() -> bool {
        true
    }

    #[inline]
    fn access(ty: TypeId) -> Access {
        if ty == TypeId::of::<T>() {
            Access::Mutable
        } else {
            Access::None
        }
    }

    #[inline]
    fn allowed_with<Q: Query>() -> bool {
        matches!(Q::access(TypeId::of::<T>()), Access::None)
    }

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype, _: u64) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch(archetype: &Archetype, _tracks: u64, epoch: u64) -> Option<FetchAlt<T>> {
        let idx = archetype.id_index(TypeId::of::<T>())?;
        let data = archetype.data(idx);
        debug_assert_eq!(data.id, TypeId::of::<T>());

        debug_assert!(*data.version.get() < epoch);
        *data.version.get() = epoch;

        Some(FetchAlt {
            epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions.cast(),
        })
    }
}

unsafe impl<T> NonTrackingQuery for Alt<T> {}
