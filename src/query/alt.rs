use core::{
    any::TypeId,
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    archetype::{chunk_idx, Archetype},
    epoch::EpochId,
    system::QueryArg,
};

use super::{Access, DefaultQuery, Fetch, IntoQuery, Query};

/// Item type that [`Alt`] yields.
/// Wraps `&mut T` and implements [`DerefMut`] to `T`.
/// Bumps component epoch on dereference.
#[derive(Debug)]
pub struct RefMut<'a, T: ?Sized> {
    pub(super) component: &'a mut T,
    pub(super) entity_epoch: &'a mut EpochId,
    pub(super) chunk_epoch: &'a Cell<EpochId>,
    pub(super) archetype_epoch: &'a Cell<EpochId>,
    pub(super) epoch: EpochId,
}

impl<T> Deref for RefMut<'_, T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        &*self.component
    }
}

impl<T> DerefMut for RefMut<'_, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        self.entity_epoch.bump_again(self.epoch);
        EpochId::bump_cell(&self.chunk_epoch, self.epoch);
        EpochId::bump_cell(&self.archetype_epoch, self.epoch);
        self.component
    }
}

/// [`Fetch`] type for the [`Alt`] query.
pub struct FetchAlt<'a, T> {
    epoch: EpochId,
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<Cell<EpochId>>,
    archetype_epoch: NonNull<Cell<EpochId>>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchAlt<'a, T>
where
    T: Send + 'a,
{
    type Item = RefMut<'a, T>;

    #[inline(always)]
    fn dangling() -> Self {
        FetchAlt {
            epoch: EpochId::start(),
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            archetype_epoch: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize);
        debug_assert!((*chunk_epoch).get().before(self.epoch));
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> RefMut<'a, T> {
        let archetype_epoch = &mut *self.archetype_epoch.as_ptr();
        let chunk_epoch = &mut *self.chunk_epochs.as_ptr().add(chunk_idx(idx) as usize);
        let entity_epoch = &mut *self.entity_epochs.as_ptr().add(idx as usize);

        debug_assert!(entity_epoch.before(self.epoch));

        RefMut {
            component: &mut *self.ptr.as_ptr().add(idx as usize),
            entity_epoch,
            chunk_epoch,
            archetype_epoch,
            epoch: self.epoch,
        }
    }
}

marker_type! {
    /// Query that yields wrapped mutable reference to specified component
    /// for each entity that has that component.
    ///
    /// Skips entities that don't have the component.
    ///
    /// Works almost as `&mut T` does.
    /// However, it does not updates entity epoch
    /// unless returned reference wrapper is dereferenced.
    pub struct Alt<T>;
}

impl<T> IntoQuery for Alt<T>
where
    T: Send + 'static,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for Alt<T>
where
    T: Send + 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        Alt
    }
}

impl<T> QueryArg for Alt<T>
where
    T: Send + 'static,
{
    #[inline(always)]
    fn new() -> Self {
        Alt
    }
}

unsafe impl<T> Query for Alt<T>
where
    T: Send + 'static,
{
    type Item<'a> = RefMut<'a, T>;
    type Fetch<'a> = FetchAlt<'a, T>;

    const MUTABLE: bool = true;

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Write)
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchAlt<'a, T> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<T>());
        let data = component.data_mut();
        debug_assert!(data.epoch.before(epoch));

        FetchAlt {
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()).cast(),
            archetype_epoch: NonNull::from(&mut data.epoch).cast(),
            marker: PhantomData,
        }
    }
}
