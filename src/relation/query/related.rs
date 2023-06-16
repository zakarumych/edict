use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Access, Fetch, ImmutablePhantomQuery, PhantomQuery},
    relation::{Relation, TargetComponent},
};

phantom_newtype! {
    /// Query for target of relation.
    ///
    /// Yields slices of origin ids for each target.
    pub struct Related<R>
}

impl<R> Related<R>
where
    R: Relation,
{
    /// Creates a new [`Related`] query.
    pub fn query() -> PhantomData<fn() -> Self> {
        PhantomQuery::query()
    }
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
    unsafe fn get_item(&mut self, idx: u32) -> &'a [EntityId] {
        let component = unsafe { &*self.ptr.as_ptr().add(idx as usize) };
        &component.origins[..]
    }
}

unsafe impl<R> PhantomQuery for Related<R>
where
    R: Relation,
{
    type Item<'a> = &'a [EntityId];
    type Fetch<'a> = FetchRelated<'a, R>;

    const MUTABLE: bool = false;

    #[inline]
    fn access(ty: TypeId) -> Option<Access> {
        if ty == TypeId::of::<TargetComponent<R>>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline]
    fn visit_archetype(archetype: &Archetype) -> bool {
        archetype.has_component(TypeId::of::<TargetComponent<R>>())
    }

    #[inline]
    unsafe fn access_archetype(_archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
        f(TypeId::of::<TargetComponent<R>>(), Access::Read)
    }

    #[inline]
    unsafe fn fetch<'a>(
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchRelated<'a, R> {
        let component = unsafe {
            archetype
                .component(TypeId::of::<TargetComponent<R>>())
                .unwrap_unchecked()
        };
        debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

        let data = unsafe { component.data() };

        FetchRelated {
            ptr: data.ptr.cast(),
            marker: PhantomData,
        }
    }
}

unsafe impl<R> ImmutablePhantomQuery for Related<R> where R: Relation {}
