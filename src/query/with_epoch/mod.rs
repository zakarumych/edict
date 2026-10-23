use crate::query::FetchCacheSlot;
use alloc::boxed::Box;
use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::{Archetype, ArchetypeComponent},
    component::ComponentInfo,
    epoch::EpochId,
    query::DefaultQuery,
    system::DefaultQueryArg,
    type_id,
};

use super::{
    Access, AsQuery, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias, fetch::Fetch,
};

mod alt;
mod read;
mod write;

/// Fetch for [`EpochOf`] epochs.
pub struct FetchEpoch<'a> {
    entity_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a [EpochId]>,
}

unsafe impl<'a> Fetch<'a> for FetchEpoch<'a> {
    type Item = EpochId;

    #[inline]
    fn dangling() -> Self {
        FetchEpoch {
            entity_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> EpochId {
        unsafe { *self.entity_epochs.as_ptr().add(idx as usize) }
    }
}

marker_type! {
    /// Query for fetching epochs of a component.
    pub struct EpochOf<T>;
}

impl<T> AsQuery for EpochOf<T>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for EpochOf<T>
where
    T: 'static,
{
    #[inline]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for EpochOf<T>
where
    T: 'static,
{
    #[inline]
    fn default_query() -> Self {
        EpochOf
    }
}

impl<T> DefaultQueryArg for EpochOf<T> where T: 'static {}

unsafe impl<T> Query for EpochOf<T>
where
    T: 'static,
{
    type Item<'a> = EpochId;
    type Fetch<'a> = FetchEpoch<'a>;

    const MUTABLE: bool = false;

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
        let _ = epoch;
        FetchEpoch {
            entity_epochs: cached.entity_epochs,
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
        unsafe { FetchEpoch::from_component(component, epoch) }
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
        Some(unsafe { FetchEpoch::from_component(component, epoch) })
    }

    #[inline]
    fn cache_visit_archetype(archetype: &Archetype) -> Option<bool> {
        Some(archetype.has_component(type_id::<T>()))
    }

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if comp.id() == type_id::<T>() {
            Ok(Some(Access::Read))
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
        f(type_id::<T>(), Access::Read);
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        epoch: EpochId,
    ) -> FetchEpoch<'a> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        unsafe { FetchEpoch::from_component(component, epoch) }
    }
}

unsafe impl<T> ImmutableQuery for EpochOf<T> where T: 'static {}
unsafe impl<T> SendQuery for EpochOf<T> where T: 'static {}

/// Query for fetching component with added epochs.
#[derive(Clone, Copy, Debug, Default)]
#[repr(transparent)]
pub struct WithEpoch<T>(pub T);

impl<'a> FetchEpoch<'a> {
    #[inline]
    unsafe fn from_component(component: &'a ArchetypeComponent, _epoch: EpochId) -> Self {
        let data = unsafe { component.data() };

        FetchEpoch {
            entity_epochs: unsafe {
                NonNull::new_unchecked(
                    Box::as_ptr(&data.entity_epochs).cast::<EpochId>() as *mut EpochId
                )
            },
            marker: PhantomData,
        }
    }
}
