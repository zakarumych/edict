use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{archetype::Archetype, epoch::EpochId};

use super::{
    fetch::Fetch,
    phantom::{ImmutablePhantomQuery, PhantomQuery},
    Access,
};

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
        *self.entity_epochs.as_ptr().add(idx as usize)
    }
}

/// Query for fetching epochs of a component.
pub struct EpochOf<T>(T);

unsafe impl<T> PhantomQuery for EpochOf<T>
where
    T: 'static,
{
    type Item<'a> = EpochId;
    type Fetch<'a> = FetchEpoch<'a>;

    const MUTABLE: bool = false;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        f(TypeId::of::<T>(), Access::Read);
    }

    #[inline]
    unsafe fn fetch<'a>(
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

unsafe impl<T> ImmutablePhantomQuery for EpochOf<T> where T: 'static {}
