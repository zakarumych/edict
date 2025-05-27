use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    entity::EntityId,
    epoch::EpochId,
    query::{AsQuery, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias},
    relation::{OriginComponent, Relation, TargetComponent},
    type_id, Access,
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

    #[inline]
    fn dangling() -> Self {
        FetchFilterRelatedBy {
            origin: EntityId::dangling(),
            ptr: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline]
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
                .targets()
                .iter()
                .any(|r| r.0 == self.origin)
        } else {
            let target_component = unsafe {
                &*self
                    .ptr
                    .cast::<TargetComponent<R>>()
                    .as_ptr()
                    .add(idx as usize)
            };
            target_component
                .origins()
                .iter()
                .any(|r| r.0 == self.origin)
        }
    }

    #[inline]
    unsafe fn get_item(&mut self, _: u32) -> () {}
}

/// Filters targets of relation with specified origin.
pub struct FilterRelatedBy<R> {
    origin: EntityId,
    phantom: PhantomData<fn() -> R>,
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

impl<R> AsQuery for FilterRelatedBy<R>
where
    R: Relation,
{
    type Query = Self;
}

impl<R> IntoQuery for FilterRelatedBy<R>
where
    R: Relation,
{
    #[inline]
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

    #[inline]
    fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
        if R::SYMMETRIC {
            if comp.id() == type_id::<OriginComponent<R>>() {
                Ok(Some(Access::Read))
            } else {
                Ok(None)
            }
        } else {
            if comp.id() == type_id::<TargetComponent<R>>() {
                Ok(Some(Access::Read))
            } else {
                Ok(None)
            }
        }
    }

    #[inline]
    fn visit_archetype(&self, archetype: &Archetype) -> bool {
        if R::SYMMETRIC {
            archetype.has_component(type_id::<OriginComponent<R>>())
        } else {
            archetype.has_component(type_id::<TargetComponent<R>>())
        }
    }

    #[inline]
    unsafe fn access_archetype(&self, _archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
        if R::SYMMETRIC {
            f(type_id::<OriginComponent<R>>(), Access::Read)
        } else {
            f(type_id::<TargetComponent<R>>(), Access::Read)
        }
    }

    #[inline]
    unsafe fn fetch<'a>(
        &self,
        _arch_idx: u32,
        archetype: &'a Archetype,
        _epoch: EpochId,
    ) -> FetchFilterRelatedBy<'a, R> {
        if R::SYMMETRIC {
            let component = unsafe {
                archetype
                    .component(type_id::<OriginComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), type_id::<OriginComponent<R>>());

            let data = unsafe { component.data() };

            FetchFilterRelatedBy {
                origin: self.origin,
                ptr: data.ptr.cast(),
                marker: PhantomData,
            }
        } else {
            let component = unsafe {
                archetype
                    .component(type_id::<TargetComponent<R>>())
                    .unwrap_unchecked()
            };
            debug_assert_eq!(component.id(), type_id::<TargetComponent<R>>());

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
unsafe impl<R> SendQuery for FilterRelatedBy<R> where R: Relation {}
