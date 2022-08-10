use core::{
    any::TypeId,
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::archetype::{chunk_idx, Archetype};

use super::{phantom::PhantomQuery, Access, Fetch, PhantomQueryFetch, Query};

/// Item type that `Alt` yields.
/// Wraps `&mut T` and implements `DerefMut` to `T`.
/// Bumps component version on dereference.
#[derive(Debug)]
pub struct RefMut<'a, T: ?Sized> {
    pub(super) component: &'a mut T,
    pub(super) entity_version: &'a mut u64,
    pub(super) chunk_version: &'a Cell<u64>,
    pub(super) archetype_version: &'a Cell<u64>,
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
        self.archetype_version.set(self.epoch);
        self.component
    }
}

/// `Fetch` type for the `Alt` query.
#[allow(missing_debug_implementations)]
pub struct FetchAlt<'a, T> {
    epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<Cell<u64>>,
    archetype_version: NonNull<Cell<u64>>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchAlt<'a, T>
where
    T: 'static,
{
    type Item = RefMut<'a, T>;

    #[inline]
    fn dangling() -> Self {
        FetchAlt {
            epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
            archetype_version: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx);
        debug_assert!((*chunk_version).get() < self.epoch);
    }

    #[inline]
    unsafe fn skip_item(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RefMut<'a, T> {
        let archetype_version = &mut *self.archetype_version.as_ptr();
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx(idx));
        let entity_version = &mut *self.entity_versions.as_ptr().add(idx);

        debug_assert!(*entity_version < self.epoch);

        RefMut {
            component: &mut *self.ptr.as_ptr().add(idx),
            entity_version,
            chunk_version,
            archetype_version,
            epoch: self.epoch,
        }
    }
}

phantom_newtype! {
    /// Query that yields wrapped mutable reference to specified component
    /// for each entity that has that component.
    ///
    /// Skips entities that don't have the component.
    ///
    /// Works almost as `&mut T` does.
    /// However, it does not updates entity version
    /// unless returned reference wrapper is dereferenced.
    pub struct Alt<T>
}

impl<'a, T> PhantomQueryFetch<'a> for Alt<T>
where
    T: 'static,
{
    type Item = RefMut<'a, T>;
    type Fetch = FetchAlt<'a, T>;
}

unsafe impl<T> PhantomQuery for Alt<T>
where
    T: 'static,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn access_any() -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn conflicts<Q>(query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(
            query.access(TypeId::of::<T>()),
            Some(Access::Read | Access::Write)
        )
    }

    #[inline]
    fn is_valid() -> bool {
        true
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: u64) -> FetchAlt<'a, T> {
        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<T>());

        debug_assert!(*data.version.get() < epoch);
        let archetype_version = NonNull::from(&mut *data.version.get());

        FetchAlt {
            epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions.cast(),
            archetype_version: archetype_version.cast(),
            marker: PhantomData,
        }
    }
}
