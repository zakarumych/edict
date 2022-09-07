use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, IntoQuery, PhantomQuery, PhantomQueryFetch},
    relation::{Relation, TargetComponent},
};

phantom_newtype! {
    /// Query for target of relation.
    ///
    /// Yields slices of origin ids for each target.
    pub struct Related<R>
}

/// Fetch type for [`Related<R>`]
pub struct FetchRelated<'a, R> {
    ptr: NonNull<TargetComponent<R>>,
    marker: PhantomData<&'a TargetComponent<R>>,
}

unsafe impl<'a, R> Fetch<'a> for FetchRelated<'a, R>
where
    R: Relation,
{
    type Item = &'a [EntityId];

    #[inline]
    fn dangling() -> Self {
        FetchRelated {
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, idx: usize) -> &'a [EntityId] {
        let component = &*self.ptr.as_ptr().add(idx);
        &component.origins[..]
    }
}

impl<'a, R> PhantomQueryFetch<'a> for Related<R>
where
    R: Relation,
{
    type Item = &'a [EntityId];
    type Fetch = FetchRelated<'a, R>;
}

impl<R> IntoQuery for Related<R>
where
    R: Relation,
{
    type Query = PhantomData<fn() -> Self>;
}

unsafe impl<R> PhantomQuery for Related<R>
where
    R: Relation,
{
    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<TargetComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn skip_archetype(archetype: &Archetype) -> bool {
        !archetype.has_component(TypeId::of::<TargetComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<TargetComponent<R>>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(archetype: &'a Archetype, _epoch: EpochId) -> FetchRelated<'a, R> {
        let component = archetype
            .component(TypeId::of::<TargetComponent<R>>())
            .unwrap_unchecked();
        debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

        let data = component.data();

        FetchRelated {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutablePhantomQuery for Related<R> where R: Relation {}
