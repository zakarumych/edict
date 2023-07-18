use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    entity::EntityId,
    epoch::EpochId,
    query::{Fetch, ImmutableQuery, IntoQuery, Query, WriteAlias},
    relation::{OriginComponent, Relation, TargetComponent},
    Access,
};

/// Fetch for the `FilterRelatedBy<R>` query.
pub struct FetchFilterRelatedBy<'a, R: Relation> {
    origin: EntityId,
    ptr: NonNull<u8>,
    marker: PhantomData<&'a R>,
}

unsafe impl<'a, R> Fetch<'a> for FetchFilterRelatedBy<'a, R>
where
    R: Relation,
{
    type Item = ();

    #[inline(always)]
    fn dangling() -> Self {
        FetchFilterRelatedBy {
            origin: EntityId::dangling(),
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn visit_item(&mut self, idx: u32) -> bool {
        if R::SYMMETRIC {
            let origin_component = unsafe {
                &*self
                    .ptr
                    .cast::<OriginComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };
            origin_component
                .relations()
                .iter()
                .any(|rt| rt.target == self.origin)
        } else {
            let target_component = unsafe {
                &*self
                    .ptr
                    .cast::<TargetComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };
            target_component.origins.contains(&self.origin)
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, _: u32) -> () {}
}

/// Filters targets of relation with specified origin.
pub struct FilterRelatedBy<R> {
    origin: EntityId,
    phantom: PhantomData<R>,
}

impl_debug!(FilterRelatedBy<R> { origin });
impl_copy!(FilterRelatedBy<R>);

impl<R> FilterRelatedBy<R> {
    /// Returns relation filter bound to one specific origin.
    pub const fn new(origin: EntityId) -> Self {
        FilterRelatedBy {
            origin,
            phantom: PhantomData,
        }
    }
}

impl<R> IntoQuery for FilterRelatedBy<R>
where
    R: Relation,
{
    type Query = Self;

    #[inline(always)]
    fn into_query(self) -> Self::Query {
        self
    }
}

unsafe impl<R> Query for FilterRelatedBy<R>
where
    R: Relation,
{
    type Item<'a> = ();
    type Fetch<'a> = FetchFilterRelatedBy<'a, R>;

    const MUTABLE: bool = false;
    const FILTERS_ENTITIES: bool = true;

    #[inline(always)]
    fn component_type_access(&self, ty: TypeId) -> Result<Option<Access>, WriteAlias> {
        if R::SYMMETRIC {
            Ok(Access::read_type::<OriginComponent<R>>(ty))
        } else {
            Ok(Access::read_type::<TargetComponent<R>>(ty))
        }
    }

    #[inline(always)]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        if R::SYMMETRIC {
            archetype.has_component(TypeId::of::<OriginComponent<R>>())
        } else {
            archetype.has_component(TypeId::of::<TargetComponent<R>>())
        }
    }

    #[inline(always)]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if R::SYMMETRIC {
            f(TypeId::of::<OriginComponent<R>>(), Access::Read)
        } else {
            f(TypeId::of::<TargetComponent<R>>(), Access::Read)
        }
    }

    #[inline(always)]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchFilterRelatedBy<'a, R> {
        if R::SYMMETRIC {
            let component = unsafe {
                archetype
                    .component(TypeId::of::<OriginComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), TypeId::of::<OriginComponent<R>>());

            let data = unsafe { component.data() };

            FetchFilterRelatedBy {
                origin: self.origin,
                ptr: data.ptr.cast(),
                marker: PhantomData,
            }
        } else {
            let component = unsafe {
                archetype
                    .component(TypeId::of::<TargetComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), TypeId::of::<TargetComponent<R>>());

            let data = unsafe { component.data() };

            FetchFilterRelatedBy {
                origin: self.origin,
                ptr: data.ptr.cast(),
                marker: PhantomData,
            }
        }
    }
}

unsafe impl<R> ImmutableQuery for FilterRelatedBy<R> where R: Relation {}
