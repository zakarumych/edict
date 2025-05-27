use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype, component::ComponentInfo, epoch::EpochId, system::QueryArg, type_id,
};

use super::{
    fetch::Fetch, Access, AsQuery, DefaultQuery, ImmutableQuery, IntoQuery, Query, SendQuery,
    WriteAlias,
};

mod read;

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

impl<T> QueryArg for EpochOf<T>
where
    T: 'static,
{
    #[inline]
    fn new() -> Self {
        EpochOf
    }
}

unsafe impl<T> Query for EpochOf<T>
where
    T: 'static,
{
    type Item<'a> = EpochId;
    type Fetch<'a> = FetchEpoch<'a>;

    const MUTABLE: bool = false;

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
        _epoch: EpochId,
    ) -> FetchEpoch<'a> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        let data = unsafe { component.data() };

        FetchEpoch {
            entity_epochs: unsafe {
                NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId)
            },
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for EpochOf<T> where T: 'static {}
unsafe impl<T> SendQuery for EpochOf<T> where T: 'static {}

#[derive(Clone, Copy, Debug, Default)]
#[repr(transparent)]
pub struct WithEpoch<T>(pub T);
