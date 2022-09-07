use core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{chunk_idx, Archetype},
    epoch::EpochId,
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::{
    alt::{Alt, RefMut},
    Access, Fetch, ImmutableQuery, IntoQuery, PhantomQuery, Query, QueryFetch,
};

/// Query over modified component.
///
/// Should be used as either [`Modified<&T>`], [`Modified<&mut T>`]
/// or [`Modified<Alt<T>>`].
///
/// This is tracking query that uses epoch lower bound to filter out entities with unmodified components.
pub struct Modified<T> {
    after_epoch: EpochId,
    marker: PhantomData<fn() -> T>,
}

impl_copy!(Modified<T>);
impl_debug!(Modified<T> { after_epoch });

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

/// [`Fetch`] type for the [`Modified<&T>`] query.
pub struct ModifiedFetchRead<'a, T> {
    after_epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
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
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(<&T as PhantomQuery>::skip_archetype(archetype), false);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data();
                !data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> ModifiedFetchRead<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        let data = component.data();

        debug_assert!(data.epoch.after(self.after_epoch));

        ModifiedFetchRead {
            after_epoch: self.after_epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_ptr() as *mut EpochId),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for Modified<&T> where T: Sync + 'static {}

/// [`Fetch`] type for the [`Modified<&mut T>`] query.
pub struct ModifiedFetchWrite<'a, T> {
    after_epoch: EpochId,
    epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
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
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(<&mut T as PhantomQuery>::skip_archetype(archetype), false);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data_mut();
                !data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> ModifiedFetchWrite<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        let data = component.data_mut();

        debug_assert!(data.epoch.after(self.after_epoch));
        data.epoch.bump(epoch);

        ModifiedFetchWrite {
            after_epoch: self.after_epoch,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            marker: PhantomData,
        }
    }
}

/// [`Fetch`] type for the [`Modified<Alt<T>>`] query.
pub struct ModifiedFetchAlt<'a, T> {
    after_epoch: EpochId,
    epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<Cell<EpochId>>,
    archetype_epoch: NonNull<Cell<EpochId>>,
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
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
        let epoch = &*self.chunk_epochs.as_ptr().add(chunk_idx);
        !epoch.get().after(self.after_epoch)
    }

    #[inline]
    unsafe fn visit_chunk(&mut self, _chunk_idx: usize) {}

    #[inline]
    unsafe fn skip_item(&mut self, idx: usize) -> bool {
        let epoch = *self.entity_epochs.as_ptr().add(idx);
        !epoch.after(self.after_epoch)
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
    fn skip_archetype(&self, archetype: &Archetype) -> bool {
        match archetype.component(TypeId::of::<T>()) {
            None => true,
            Some(component) => unsafe {
                debug_assert_eq!(<Alt<T> as PhantomQuery>::skip_archetype(archetype), false);

                debug_assert_eq!(component.id(), TypeId::of::<T>());
                let data = component.data_mut();
                !data.epoch.after(self.after_epoch)
            },
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &mut self,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> ModifiedFetchAlt<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let data = component.data_mut();

        debug_assert!(data.epoch.after(self.after_epoch));
        debug_assert!(data.epoch.before(epoch));

        ModifiedFetchAlt {
            after_epoch: self.after_epoch,
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()).cast(),
            archetype_epoch: NonNull::from(&mut data.epoch).cast(),
            marker: PhantomData,
        }
    }
}

pub struct ModifiedCache<T> {
    after_epoch: EpochId,
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ModifiedCache<T> {
    fn default() -> Self {
        ModifiedCache {
            after_epoch: EpochId::start(),
            marker: PhantomData,
        }
    }
}

impl<'a, T> QueryArgGet<'a> for ModifiedCache<&'static T>
where
    T: Sync + 'static,
{
    type Arg = Modified<&'a T>;
    type Query = Modified<&'a T>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<&'a T> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<&'static T>
where
    T: Sync + 'static,
{
    fn access_component(&self, id: TypeId) -> Option<Access> {
        <&T as PhantomQuery>::access(id)
    }

    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        <&T as PhantomQuery>::skip_archetype(archetype)
    }
}

impl<'a, T> QueryArg for Modified<&'a T>
where
    T: Sync + 'static,
{
    type Cache = ModifiedCache<&'static T>;
}

impl<'a, T> QueryArgGet<'a> for ModifiedCache<&'static mut T>
where
    T: Send + 'static,
{
    type Arg = Modified<&'a mut T>;
    type Query = Modified<&'a mut T>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<&'a mut T> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<&'static mut T>
where
    T: Send + 'static,
{
    fn access_component(&self, id: TypeId) -> Option<Access> {
        <&mut T as PhantomQuery>::access(id)
    }

    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        <&mut T as PhantomQuery>::skip_archetype(archetype)
    }
}

impl<'a, T> QueryArg for Modified<&'a mut T>
where
    T: Send + 'static,
{
    type Cache = ModifiedCache<&'static mut T>;
}

impl<'a, T> QueryArgGet<'a> for ModifiedCache<Alt<T>>
where
    T: Send + 'static,
{
    type Arg = Modified<Alt<T>>;
    type Query = Modified<Alt<T>>;

    #[inline]
    fn get(&mut self, world: &'a World) -> Modified<Alt<T>> {
        let after_epoch = core::mem::replace(&mut self.after_epoch, world.epoch());

        Modified {
            after_epoch,
            marker: PhantomData,
        }
    }
}

impl<T> QueryArgCache for ModifiedCache<Alt<T>>
where
    T: Send + 'static,
{
    fn access_component(&self, id: TypeId) -> Option<Access> {
        <&mut T as PhantomQuery>::access(id)
    }

    fn skips_archetype(&self, archetype: &Archetype) -> bool {
        <&mut T as PhantomQuery>::skip_archetype(archetype)
    }
}

impl<'a, T> QueryArg for Modified<Alt<T>>
where
    T: Send + 'static,
{
    type Cache = ModifiedCache<Alt<T>>;
}
