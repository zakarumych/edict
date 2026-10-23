use crate::query::FetchCacheSlot;
use alloc::boxed::Box;
use core::{
    any::TypeId,
    cell::Cell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    archetype::{Archetype, ArchetypeComponent, chunk_idx},
    component::ComponentInfo,
    epoch::EpochId,
    query::DefaultQuery,
    system::DefaultQueryArg,
    type_id,
};

use super::{Access, AsQuery, Fetch, IntoQuery, Query, SendQuery, WriteAlias};

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

    #[inline]
    fn deref(&self) -> &T {
        &*self.component
    }
}

impl<T> DerefMut for RefMut<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        Self::bump(self);
        self.component
    }
}

impl<T> RefMut<'_, T> {
    /// Force-bump epoch of the referenced entity.
    pub fn bump(this: &mut Self) {
        this.entity_epoch.bump_again(this.epoch);
        EpochId::bump_cell(this.chunk_epoch, this.epoch);
        EpochId::bump_cell(this.archetype_epoch, this.epoch);
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
    T: 'a,
{
    type Item = RefMut<'a, T>;

    #[inline]
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

    #[inline]
    unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx as usize) };
        debug_assert!((*chunk_epoch).get().before(self.epoch));
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> RefMut<'a, T> {
        let archetype_epoch = unsafe { &mut *self.archetype_epoch.as_ptr() };
        let chunk_epoch = unsafe { &mut *self.chunk_epochs.as_ptr().add(chunk_idx(idx) as usize) };
        let entity_epoch = unsafe { &mut *self.entity_epochs.as_ptr().add(idx as usize) };

        debug_assert!(entity_epoch.before(self.epoch));

        RefMut {
            component: unsafe { &mut *self.ptr.as_ptr().add(idx as usize) },
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

impl<T> AsQuery for Alt<T>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for Alt<T>
where
    T: 'static,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for Alt<T>
where
    T: 'static,
{
    #[inline]
    fn default_query() -> Self {
        Alt
    }
}

impl<T> DefaultQueryArg for Alt<T> where T: Send + 'static {}

unsafe impl<T> Query for Alt<T>
where
    T: 'static,
{
    type Item<'a> = RefMut<'a, T>;
    type Fetch<'a> = FetchAlt<'a, T>;

    const MUTABLE: bool = true;

    const SUPPORTS_FETCH_REUSE: bool = true;
    const FETCH_REUSE_MATCHES_STABLE: bool = true;

    #[inline]
    unsafe fn reuse_fetch<'cached, 'a>(
        &self,
        cached: &mut Self::Fetch<'cached>,
        _arch_idx: u32,
        _archetype: &'a Archetype,
        epoch: EpochId,
    ) -> Self::Fetch<'a> {
        debug_assert!(
            unsafe { cached.archetype_epoch.as_ref() }
                .get()
                .before(epoch)
        );
        cached.epoch = epoch;
        FetchAlt {
            ptr: cached.ptr,
            entity_epochs: cached.entity_epochs,
            chunk_epochs: cached.chunk_epochs,
            epoch,
            archetype_epoch: cached.archetype_epoch,
            marker: PhantomData,
        }
    }

    const FETCH_CACHE_SLOTS: usize = 1;

    #[inline]
    fn prepare_fetch_cache(archetype: &Archetype, slots: &mut [FetchCacheSlot]) {
        slots[0] = FetchCacheSlot::component(archetype, type_id::<T>());
    }

    #[inline]
    unsafe fn fetch_cached<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
        slots: &[FetchCacheSlot],
    ) -> Self::Fetch<'a> {
        let component = unsafe { slots[0].get(archetype).unwrap_unchecked() };
        unsafe { FetchAlt::from_component(component, epoch) }
    }

    #[inline]
    unsafe fn fetch_optional_cached<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
        slots: &[FetchCacheSlot],
    ) -> Option<Self::Fetch<'a>> {
        let component = unsafe { slots[0].get(archetype) }?;
        Some(unsafe { FetchAlt::from_component(component, epoch) })
    }
    const FILTERS_ENTITIES: bool = false;

    #[inline]
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        Some(archetype.has_component(type_id::<T>()))
    }

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<T>() {
            Ok(Some(Access::Write))
        } else {
            Ok(None)
        }
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        archetype.has_component(type_id::<T>())
    }
    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(type_id::<T>(), Access::Write)
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchAlt<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        unsafe { FetchAlt::from_component(component, epoch) }
    }
}

unsafe impl<T> SendQuery for Alt<T> where T: Send + 'static {}

impl<'a, T: 'static> FetchAlt<'a, T> {
    #[inline]
    unsafe fn from_component(component: &'a ArchetypeComponent, epoch: EpochId) -> Self {
        debug_assert_eq!(component.id(), type_id::<T>());
        let data = unsafe { component.data_mut() };
        debug_assert!(data.epoch.before(epoch));

        FetchAlt {
            epoch,
            ptr: data.ptr.cast(),
            entity_epochs: unsafe {
                NonNull::new_unchecked(Box::as_mut_ptr(&mut data.entity_epochs).cast::<EpochId>())
            },
            chunk_epochs: unsafe {
                NonNull::new_unchecked(Box::as_mut_ptr(&mut data.chunk_epochs).cast::<EpochId>())
            }
            .cast(),
            archetype_epoch: component.epoch_ptr().cast(),
            marker: PhantomData,
        }
    }
}
