use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use atomicell::{borrow::AtomicBorrowMut, RefMut};

use crate::{
    archetype::Archetype,
    epoch::EpochId,
    system::{QueryArg, QueryArgCache, QueryArgGet},
    world::World,
};

use super::{phantom::PhantomQuery, Access, Fetch, IntoQuery, PhantomQueryFetch, Query};

/// `Fetch` type for the `&mut T` query.
pub struct FetchWrite<'a, T> {
    ptr: NonNull<T>,
    entity_epochs: NonNull<EpochId>,
    chunk_epochs: NonNull<EpochId>,
    epoch: EpochId,
    _borrow: AtomicBorrowMut<'a>,
    marker: PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T> Fetch<'a> for FetchWrite<'a, T>
where
    T: Send + 'a,
{
    type Item = &'a mut T;

    #[inline]
    fn dangling() -> Self {
        FetchWrite {
            ptr: NonNull::dangling(),
            entity_epochs: NonNull::dangling(),
            chunk_epochs: NonNull::dangling(),
            epoch: EpochId::start(),
            _borrow: AtomicBorrowMut::dummy(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn skip_chunk(&mut self, _: usize) -> bool {
        false
    }

    #[inline]
    unsafe fn skip_item(&mut self, _: usize) -> bool {
        false
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

impl<T> IntoQuery for &mut T
where
    T: Send + 'static,
{
    type Query = PhantomData<Self>;
}

impl<'a, T> PhantomQueryFetch<'a> for &mut T
where
    T: Send + 'a,
{
    type Item = &'a mut T;
    type Fetch = FetchWrite<'a, T>;
}

unsafe impl<T> PhantomQuery for &mut T
where
    T: Send + 'static,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline]
    fn access_any() -> Option<Access> {
        Some(Access::Write)
    }

    #[inline]
    fn conflicts<Q>(query: &Q) -> bool
    where
        Q: Query,
    {
        matches!(
            query.access(TypeId::of::<T>()),
            Some(Access::Read | Access::Write)
        )
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.contains_id(TypeId::of::<T>())
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: EpochId) -> FetchWrite<'a, T> {
        debug_assert_ne!(archetype.len(), 0, "Empty archetypes must be skipped");

        let idx = archetype.id_index(TypeId::of::<T>()).unwrap_unchecked();
        let component = archetype.component(idx);
        debug_assert_eq!(component.id(), TypeId::of::<T>());

        let (data, borrow) = RefMut::into_split(component.data.borrow_mut());
        data.epoch.bump(epoch);

        FetchWrite {
            ptr: data.ptr.cast(),
            entity_epochs: NonNull::new_unchecked(data.entity_epochs.as_mut_ptr()),
            chunk_epochs: NonNull::new_unchecked(data.chunk_epochs.as_mut_ptr()),
            epoch,
            _borrow: borrow,
            marker: PhantomData,
        }
    }
}

/// Returns query that yields mutable reference to specified component
/// for each entity that has that component.
///
/// Skips entities that don't have the component.
pub fn write<T>() -> PhantomData<&'static mut T>
where
    T: Send,
{
    PhantomData
}

// impl<'a, T> QueryArg for &'a mut T
// where
//     T: Send + 'static,
// {
//     type Cache = PhantomData<&'static mut T>;
// }

// impl<T> QueryArgCache for PhantomData<&'static mut T>
// where
//     T: Send + 'static,
// {
//     fn access_component(&self, id: TypeId) -> Option<Access> {
//         if id == TypeId::of::<T>() {
//             Some(Access::Read)
//         } else {
//             None
//         }
//     }

//     fn skips_archetype(&self, archetype: &Archetype) -> bool {
//         !archetype.contains_id(TypeId::of::<T>())
//     }
// }
// impl<'a, T> QueryArgGet<'a> for PhantomData<&'static mut T>
// where
//     T: Send + 'static,
// {
//     type Arg = &'a mut T;
//     type Query = PhantomData<&'a mut T>;

//     fn get(&mut self, _world: &World) -> PhantomData<&'a mut T> {
//         PhantomData
//     }
// }
