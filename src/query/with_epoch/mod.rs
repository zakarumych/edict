use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, epoch::EpochId, system::QueryArg};

use super::{fetch::Fetch, Access, DefaultQuery, ImmutableQuery, IntoQuery, Query};

/// Fetch for [`EpochOf`] epochs.
pub struct FetchEpoch<'a> {
    entity_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a [EpochId]>,
}

unsafe impl<'a> Fetch<'a> for FetchEpoch<'a> {
    type Item = EpochId;

    #[inline(always)]
    fn dangling() -> Self {
        FetchEpoch {
            entity_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> EpochId {
        *self.entity_epochs.as_ptr().add(idx as usize)
    }
}

marker_type! {
    /// Query for fetching epochs of a component.
    pub struct EpochOf<T>;
}

impl<T> IntoQuery for EpochOf<T>
where
    T: 'static,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for EpochOf<T>
where
    T: 'static,
{
    #[inline(always)]
    fn default_query() -> Self {
        EpochOf
    }
}

impl<T> QueryArg for EpochOf<T>
where
    T: 'static,
{
    #[inline(always)]
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

    #[inline(always)]
    fn access(&self, ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Read)
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
        f(TypeId::of::<T>(), Access::Read);
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchEpoch<'a> {
        let component = archetype.component(TypeId::of::<T>()).unwrap_unchecked();
        let data = component.data();

        FetchEpoch {
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId),
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for EpochOf<T> where T: 'static {}
