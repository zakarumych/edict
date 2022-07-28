use core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{chunk_idx, Archetype},
    component::Component,
};

use super::{
    alt::{Alt, RefMut},
    Access, Fetch, ImmutableQuery, PhantomQuery, Query,
};

/// Query over modified component.
///
/// Should be used as either `Modified<&T>`, `Modified<&mut T>`
/// or `Modified<Alt<T>>`.
///
/// This is tracking query that requires providing subscriber's
/// `Tracks` to skip components that are not modified since the last time
/// that `Tracks` instance was used.
pub struct Modified<T> {
    epoch: u64,
    marker: PhantomData<fn() -> T>,
}

phantom_copy!(Modified<T>);
phantom_debug!(Modified<T> { epoch });

impl<T> Modified<T> {
    /// Creates new `Modified` query.
    /// Provide `epoch` id is used to skip components that are not modified since the epoch id.
    pub fn new(epoch: u64) -> Self {
        Modified {
            epoch,
            marker: PhantomData,
        }
    }
}

/// `Fetch` type for the `Modified<&T>` query.
#[allow(missing_debug_implementations)]
pub struct ModifiedFetchRead<T> {
    epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchRead<T>
where
    T: 'a,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchRead {
            epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let version = *self.chunk_versions.as_ptr().add(chunk_idx);
        version <= self.epoch
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let version = *self.entity_versions.as_ptr().add(idx);
        version <= self.epoch
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _: usize) {}

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a T {
        &*self.ptr.as_ptr().add(idx)
    }
}

unsafe impl<T> Query for Modified<&T>
where
    T: Component,
{
    type Fetch = ModifiedFetchRead<T>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <&T as PhantomQuery>::access(ty)
    }

    #[inline]
    fn conflicts<U>(&self, other: &U) -> bool
    where
        U: Query,
    {
        <&T as PhantomQuery>::conflicts(other)
    }

    #[inline]
    fn is_valid(&self) -> bool {
        <&T as PhantomQuery>::is_valid()
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(<&T as PhantomQuery>::skip_archetype(archetype), false);

                let data = archetype.data(idx);
                debug_assert_eq!(data.id(), TypeId::of::<T>());
                *data.version.get() < self.epoch
            },
        }
    }

    #[inline]
    unsafe fn fetch(&mut self, archetype: &Archetype, _epoch: u64) -> ModifiedFetchRead<T> {
        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let data = archetype.data(idx);

        ModifiedFetchRead {
            epoch: self.epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions,
        }
    }
}

unsafe impl<T> ImmutableQuery for Modified<&T> where T: Component {}

/// `Fetch` type for the `Modified<&mut T>` query.
#[allow(missing_debug_implementations)]
pub struct ModifiedFetchWrite<T> {
    epoch: u64,
    new_epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchWrite<T>
where
    T: 'a,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchWrite {
            epoch: 0,
            new_epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let version = *self.chunk_versions.as_ptr().add(chunk_idx);
        version <= self.epoch
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let version = *self.entity_versions.as_ptr().add(idx);
        version <= self.epoch
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx);

        debug_assert!(*chunk_version < self.new_epoch);
        *chunk_version = self.new_epoch;
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a mut T {
        let entity_version = &mut *self.entity_versions.as_ptr().add(idx);

        debug_assert!(*entity_version < self.new_epoch);
        *entity_version = self.new_epoch;

        &mut *self.ptr.as_ptr().add(idx)
    }
}

unsafe impl<T> Query for Modified<&mut T>
where
    T: Component,
{
    type Fetch = ModifiedFetchWrite<T>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <&mut T as PhantomQuery>::access(ty)
    }

    #[inline]
    fn conflicts<Q>(&self, other: &Q) -> bool
    where
        Q: Query,
    {
        <&mut T as PhantomQuery>::conflicts::<Q>(other)
    }

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(<&mut T as PhantomQuery>::skip_archetype(archetype), false);

                let data = archetype.data(idx);
                debug_assert_eq!(data.id(), TypeId::of::<T>());
                *data.version.get() < self.epoch
            },
        }
    }

    #[inline]
    unsafe fn fetch(&mut self, archetype: &Archetype, new_epoch: u64) -> ModifiedFetchWrite<T> {
        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let data = archetype.data(idx);

        debug_assert!(*data.version.get() < new_epoch);
        *data.version.get() = new_epoch;

        ModifiedFetchWrite {
            epoch: self.epoch,
            new_epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions,
        }
    }
}

#[allow(missing_debug_implementations)]
/// `Fetch` type for the `Modified<Alt<T>>` query.
pub struct ModifiedFetchAlt<T> {
    epoch: u64,
    new_epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<Cell<u64>>,
    archetype_version: NonNull<Cell<u64>>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchAlt<T>
where
    T: 'a,
{
    type Item = RefMut<'a, T>;

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchAlt {
            epoch: 0,
            new_epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
            archetype_version: NonNull::dangling(),
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let version = &*self.chunk_versions.as_ptr().add(chunk_idx);
        version.get() <= self.epoch
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx);
        debug_assert!(chunk_version.get() < self.new_epoch);
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let version = *self.entity_versions.as_ptr().add(idx);
        version <= self.epoch
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RefMut<'a, T> {
        let archetype_version = &mut *self.archetype_version.as_ptr();
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx(idx));
        let entity_version = &mut *self.entity_versions.as_ptr().add(idx);

        debug_assert!(*entity_version < self.new_epoch);

        RefMut {
            component: &mut *self.ptr.as_ptr().add(idx),
            entity_version,
            chunk_version,
            archetype_version,
            epoch: self.new_epoch,
        }
    }
}

unsafe impl<T> Query for Modified<Alt<T>>
where
    T: Component,
{
    type Fetch = ModifiedFetchAlt<T>;

    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Alt<T> as PhantomQuery>::access(ty)
    }

    #[inline]
    fn conflicts<Q>(&self, other: &Q) -> bool
    where
        Q: Query,
    {
        <Alt<T> as PhantomQuery>::conflicts::<Q>(other)
    }

    #[inline]
    fn is_valid(&self) -> bool {
        true
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(<Alt<T> as PhantomQuery>::skip_archetype(archetype), false);

                let data = archetype.data(idx);
                debug_assert_eq!(data.id(), TypeId::of::<T>());
                *data.version.get() < self.epoch
            },
        }
    }

    #[inline]
    unsafe fn fetch(&mut self, archetype: &Archetype, new_epoch: u64) -> ModifiedFetchAlt<T> {
        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let data = archetype.data(idx);
        debug_assert_eq!(data.id(), TypeId::of::<T>());

        debug_assert!(*data.version.get() >= self.epoch);
        debug_assert!(*data.version.get() < new_epoch);

        let archetype_version = NonNull::from(&mut *data.version.get());

        ModifiedFetchAlt {
            epoch: self.epoch,
            new_epoch,
            ptr: data.ptr.cast(),
            entity_versions: data.entity_versions,
            chunk_versions: data.chunk_versions.cast(),
            archetype_version: archetype_version.cast(),
        }
    }
}
