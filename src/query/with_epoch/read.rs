use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    epoch::EpochId,
    query::{
        Access, AsQuery, DefaultQuery, Fetch, ImmutableQuery, IntoQuery, Query, Read, SendQuery,
        WriteAlias,
    },
    system::QueryArg,
    type_id,
};

use super::WithEpoch;

/// [`Fetch`] type for the [`WithEpoch<&T>`] query.
pub struct WithEpochFetchRead<'a, T> {
    ptr: NonNull<T>,
    epochs: NonNull<EpochId>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for WithEpochFetchRead<'a, T>
where
    T: 'a,
{
    type Item = (&'a T, EpochId);

    #[inline]
    fn dangling() -> Self {
        WithEpochFetchRead {
            ptr: NonNull::dangling(),
            epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: u32) -> (&'a T, EpochId) {
        let epoch = unsafe { *self.epochs.as_ptr().add(idx as usize) };
        let item = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        (item, epoch)
    }
}

impl<T> AsQuery for WithEpoch<&T>
where
    T: 'static,
{
    type Query = WithEpoch<Read<T>>;
}

impl<T> DefaultQuery for WithEpoch<&T>
where
    T: 'static,
{
    fn default_query() -> Self::Query {
        WithEpoch(Read)
    }
}

impl<T> AsQuery for WithEpoch<Read<T>>
where
    T: 'static,
{
    type Query = Self;
}

impl<T> IntoQuery for WithEpoch<Read<T>>
where
    T: 'static,
{
    fn into_query(self) -> Self {
        self
    }
}

impl<T> DefaultQuery for WithEpoch<Read<T>>
where
    T: 'static,
{
    fn default_query() -> Self {
        WithEpoch(Read)
    }
}

impl<T> QueryArg for WithEpoch<Read<T>>
where
    T: Sync + 'static,
{
    fn new() -> Self {
        WithEpoch(Read)
    }
}

unsafe impl<T> Query for WithEpoch<Read<T>>
where
    T: 'static,
{
    type Item<'a> = (&'a T, EpochId);
    type Fetch<'a> = WithEpochFetchRead<'a, T>;

    const MUTABLE: bool = false;
    const FILTERS_ENTITIES: bool = false;

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        self.0.component_access(comp)
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        self.0.visit_archetype(archetype)
    }

    #[inline]
    unsafe fn access_archetype(&self, archetype: &Archetype, f: impl FnMut(TypeId, Access)) {
        unsafe {
            self.0.access_archetype(archetype, f);
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> WithEpochFetchRead<'a, T> {
        let component = unsafe { archetype.component(type_id::<T>()).unwrap_unchecked() };
        debug_assert_eq!(component.id(), type_id::<T>());

        let data = unsafe { component.data() };

        WithEpochFetchRead {
            ptr: data.ptr.cast(),
            epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId) },
            marker: PhantomData,
        }
    }
}

unsafe impl<T> ImmutableQuery for WithEpoch<Read<T>> where T: 'static {}
unsafe impl<T> SendQuery for WithEpoch<Read<T>> where T: Sync + 'static {}
