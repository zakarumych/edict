use core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::{AtomicBorrow, AtomicBorrowMut};

use crate::archetype::{chunk_idx, Archetype};

use super::{
    alt::{Alt, RefMut},
    Access, Fetch, ImmutableQuery, PhantomQuery, Query, QueryFetch,
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
pub struct ModifiedFetchRead<'a, T> {
    epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
    _borrow: AtomicBorrow<'a>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchRead<'a, T>
where
    T: Sync + 'a,
{
    type Item = &'a T;

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchRead {
            epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
            _borrow: AtomicBorrow::dummy(),
            marker: PhantomData,
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

impl<'a, T> QueryFetch<'a> for Modified<&T>
where
    T: Sync + 'a,
{
    type Item = &'a T;
    type Fetch = ModifiedFetchRead<'a, T>;
}

unsafe impl<T> Query for Modified<&T>
where
    T: Sync + 'static,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <&T as PhantomQuery>::access(ty)
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        <&T as PhantomQuery>::access_any()
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
    fn skip_archetype_unconditionally(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(
                    <&T as PhantomQuery>::skip_archetype_unconditionally(archetype),
                    false
                );

                let component = archetype.component(idx);
                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data.borrow();
                data.version < self.epoch
            },
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: u64,
    ) -> ModifiedFetchRead<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let component = archetype.component(idx);
        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        ModifiedFetchRead {
            epoch: self.epoch,
            ptr: data.ptr.cast(),
            entity_versions: NonNull::new_unchecked(data.entity_versions.as_ptr() as *mut u64),
            chunk_versions: NonNull::new_unchecked(data.chunk_versions.as_ptr() as *mut u64),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Modified<&T> where T: Sync + 'static {}

/// `Fetch` type for the `Modified<&mut T>` query.
pub struct ModifiedFetchWrite<'a, T> {
    epoch: u64,
    new_epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<u64>,
    _borrow: AtomicBorrowMut<'a>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchWrite<'a, T>
where
    T: Send + 'a,
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
            _borrow: AtomicBorrowMut::dummy(),
            marker: PhantomData,
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

impl<'a, T> QueryFetch<'a> for Modified<&mut T>
where
    T: Send + 'a,
{
    type Item = &'a mut T;
    type Fetch = ModifiedFetchWrite<'a, T>;
}

unsafe impl<T> Query for Modified<&mut T>
where
    T: Send + 'static,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <&mut T as PhantomQuery>::access(ty)
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        <&mut T as PhantomQuery>::access_any()
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
    fn skip_archetype_unconditionally(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(
                    <&mut T as PhantomQuery>::skip_archetype_unconditionally(archetype),
                    false
                );

                let component = archetype.component(idx);
                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data.borrow();
                data.version < self.epoch
            },
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        new_epoch: u64,
    ) -> ModifiedFetchWrite<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let component = archetype.component(idx);
        let mut data = component.data.borrow_mut();

        debug_assert!(data.version < new_epoch);
        data.version = new_epoch;

        let (data, borrow) = atomicell::RefMut::into_split(data);

        ModifiedFetchWrite {
            epoch: self.epoch,
            new_epoch,
            ptr: data.ptr.cast(),
            entity_versions: NonNull::new_unchecked(data.entity_versions.as_mut_ptr()),
            chunk_versions: NonNull::new_unchecked(data.chunk_versions.as_mut_ptr()),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

/// `Fetch` type for the `Modified<Alt<T>>` query.
pub struct ModifiedFetchAlt<'a, T> {
    track_epoch: u64,
    epoch: u64,
    ptr: NonNull<T>,
    entity_versions: NonNull<u64>,
    chunk_versions: NonNull<Cell<u64>>,
    archetype_version: NonNull<Cell<u64>>,
    _borrow: AtomicBorrowMut<'a>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for ModifiedFetchAlt<'a, T>
where
    T: Send + 'a,
{
    type Item = RefMut<'a, T>;

    #[inline]
    fn dangling() -> Self {
        ModifiedFetchAlt {
            track_epoch: 0,
            epoch: 0,
            ptr: NonNull::dangling(),
            entity_versions: NonNull::dangling(),
            chunk_versions: NonNull::dangling(),
            archetype_version: NonNull::dangling(),
            _borrow: AtomicBorrowMut::dummy(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let version = &*self.chunk_versions.as_ptr().add(chunk_idx);
        version.get() <= self.track_epoch
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_version = &mut *self.chunk_versions.as_ptr().add(chunk_idx);
        debug_assert!(chunk_version.get() < self.epoch);
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let version = *self.entity_versions.as_ptr().add(idx);
        version <= self.track_epoch
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

impl<'a, T> QueryFetch<'a> for Modified<Alt<T>>
where
    T: Send + 'a,
{
    type Item = RefMut<'a, T>;
    type Fetch = ModifiedFetchAlt<'a, T>;
}

unsafe impl<T> Query for Modified<Alt<T>>
where
    T: Send + 'static,
{
    #[inline]
    fn access(&self, ty: TypeId) -> Option<Access> {
        <Alt<T> as PhantomQuery>::access(ty)
    }

    #[inline]
    fn access_any(&self) -> Option<Access> {
        <Alt<T> as PhantomQuery>::access_any()
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
    fn skip_archetype_unconditionally(&self, archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(
                    <Alt<T> as PhantomQuery>::skip_archetype_unconditionally(archetype),
                    false
                );

                let component = archetype.component(idx);
                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data.borrow();
                data.version < self.epoch
            },
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        new_epoch: u64,
    ) -> ModifiedFetchAlt<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data.borrow_mut();

        debug_assert!(data.version >= self.epoch);
        debug_assert!(data.version < new_epoch);

        let (data, borrow) = atomicell::RefMut::into_split(data);

        ModifiedFetchAlt {
            track_epoch: self.epoch,
            epoch: new_epoch,
            ptr: data.ptr.cast(),
            entity_versions: NonNull::new_unchecked(data.entity_versions.as_mut_ptr()),
            chunk_versions: NonNull::new_unchecked(data.chunk_versions.as_mut_ptr()).cast(),
            archetype_version: NonNull::from(&mut data.version).cast(),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}
