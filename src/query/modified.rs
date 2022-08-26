use core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull};

use atomicell::borrow::{AtomicBorrow, AtomicBorrowMut};

use crate::{
    archetype::{chunk_idx, Archetype},
    epoch::EpochId,
};

use super::{
    alt::{Alt, RefMut},
    Access, Fetch, ImmutableQuery, IntoQuery, PhantomQuery, Query, QueryFetch,
};

/// Query over modified component.
///
/// Should be used as either `Modified<&T>`, `Modified<&mut T>`
/// or `Modified<Alt<T>>`.
///
/// This is tracking query that uses epoch lower bound to filter out entities with unmodified components.
pub struct Modified<T> {
    after_epoch: EpochId,
    marker: PhantomData<fn() -> T>,
}

phantom_copy!(Modified<T>);
phantom_debug!(Modified<T> { after_epoch });

impl<T> Modified<T> {
    /// Creates new `Modified` query.
    /// Provide `after_epoch` id is used to skip components that are last modified not after this epoch.
    pub fn new(after_epoch: EpochId) -> Self {
        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

/// `Fetch` type for the `Modified<&T>` query.
pub struct ModifiedFetchRead<'a, T> {
    after_epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
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
            after_epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            _borrow: AtomicBorrow::dummy(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let chunk_epoch = *self.chunk_epochs.as_ptr().add(chunk_idx);
        !chunk_epoch.after(self.after_epoch)
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx);
        !epoch.after(self.after_epoch)
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

impl<T> IntoQuery for Modified<&T>
where
    T: Sync + 'static,
{
    type Query = Self;
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
        true
    }

    #[inline]
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(<&T as PhantomQuery>::skip_archetype(archetype), false);

                let component = archetype.component(idx);
                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data.borrow();
                self.after_epoch.before(data.epoch)
            },
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> ModifiedFetchRead<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let component = archetype.component(idx);
        let (data, borrow) = atomicell::Ref::into_split(component.data.borrow());

        debug_assert!(data.epoch.after(self.after_epoch));

        ModifiedFetchRead {
            after_epoch: self.after_epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_ptr() as *mut EpochId),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Modified<&T> where T: Sync + 'static {}

/// `Fetch` type for the `Modified<&mut T>` query.
pub struct ModifiedFetchWrite<'a, T> {
    after_epoch: EpochId,
    epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
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
            after_epoch: EpochId::start(),
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            _borrow: AtomicBorrowMut::dummy(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let chunk_epoch = *self.chunk_epochs.as_ptr().add(chunk_idx);
        self.after_epoch.before(chunk_epoch)
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx);
        self.after_epoch.before(epoch)
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_epoch.bump(self.epoch);
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a mut T {
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx);
        entity_epoch.bump(self.epoch);

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

impl<T> IntoQuery for Modified<&mut T>
where
    T: Send + 'static,
{
    type Query = Self;
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
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(<&mut T as PhantomQuery>::skip_archetype(archetype), false);

                let component = archetype.component(idx);
                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data.borrow();
                self.after_epoch.before(data.epoch)
            },
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> ModifiedFetchWrite<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let component = archetype.component(idx);
        let mut data = component.data.borrow_mut();

        debug_assert!(data.epoch.after(self.after_epoch));
        data.epoch.bump(epoch);

        let (data, borrow) = atomicell::RefMut::into_split(data);

        ModifiedFetchWrite {
            after_epoch: self.after_epoch,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

/// `Fetch` type for the `Modified<Alt<T>>` query.
pub struct ModifiedFetchAlt<'a, T> {
    after_epoch: EpochId,
    epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<Cell<EpochId>>,
    archetype_epoch: NonNull<Cell<EpochId>>,
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
            after_epoch: EpochId::start(),
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            archetype_epoch: NonNull::dangling(),
            _borrow: AtomicBorrowMut::dummy(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let epoch = &*self.chunk_epochs.as_ptr().add(chunk_idx);
        self.after_epoch.before(epoch.get())
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx);
        chunk_epoch.get().before(self.epoch);
    }

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx);
        self.after_epoch.before(epoch)
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> RefMut<'a, T> {
        let archetype_epoch = &mut *self.archetype_epoch.as_ptr();
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx(idx));
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx);

        debug_assert!(entity_epoch.before(self.epoch));

        RefMut {
            component: &mut *self.ptr.as_ptr().add(idx),
            entity_epoch,
            chunk_epoch,
            archetype_epoch,
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

impl<T> IntoQuery for Modified<Alt<T>>
where
    T: Send + 'static,
{
    type Query = Self;
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
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.id_index(TypeId::of::<T>()) {
            None => true,
            Some(idx) => unsafe {
                debug_assert_eq!(<Alt<T> as PhantomQuery>::skip_archetype(archetype), false);

                let component = archetype.component(idx);
                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data.borrow();
                self.after_epoch.after(data.epoch)
            },
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> ModifiedFetchAlt<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data.borrow_mut();

        debug_assert!(data.epoch.after(self.after_epoch));
        debug_assert!(data.epoch.before(epoch));

        let (data, borrow) = atomicell::RefMut::into_split(data);

        ModifiedFetchAlt {
            after_epoch: self.after_epoch,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()).cast(),
            archetype_epoch: NonNull::from(&mut data.epoch).cast(),
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}
