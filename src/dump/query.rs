use core::{any::TypeId, marker::PhantomData, ptr::NonNull};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    entity::EntityLoc,
    epoch::EpochId,
    query::{
        AsQuery, Entities, EntitiesFetch, Fetch, ImmutableQuery, IntoQuery, Query, SendQuery,
        WriteAlias,
    },
    type_id, Access,
};

/// Query result per component.
pub enum DumpItem<'a, T> {
    /// Component is missing.
    Missing,

    /// Component is present and modified.
    Modified(&'a T),

    /// Component is present and unmodified.
    Unmodified,
}

impl<'a, T> Clone for DumpItem<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Copy for DumpItem<'a, T> {}

/// Query for fetching components to serialize.
pub(super) struct DumpQuery<T> {
    after_epoch: EpochId,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for DumpQuery<T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for DumpQuery<T> {}

impl<T> DumpQuery<T> {
    #[inline(always)]
    pub fn new(after_epoch: EpochId) -> Self {
        DumpQuery {
            after_epoch,
            marker: PhantomData,
        }
    }
}

pub(super) struct DumpFetch<'a, T> {
    after_epoch: EpochId,
    ptr: Option<NonNull<T>>,
    entity_epochs: NonNull<EpochId>,
    marker: PhantomData<&'a [T]>,
}

unsafe impl<'a, T> Fetch<'a> for DumpFetch<'a, T>
where
    T: 'a,
{
    type Item = DumpItem<'a, T>;

    #[inline(always)]
    fn dangling() -> Self {
        DumpFetch {
            after_epoch: EpochId::start(),
            ptr: None,
            entity_epochs: NonNull::dangling(),
            marker: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn get_item(&mut self, idx: u32) -> DumpItem<'a, T> {
        match self.ptr {
            None => DumpItem::Missing,
            Some(ptr) => {
                let epoch = unsafe { *self.entity_epochs.as_ptr().add(idx as usize) };
                if epoch.after(self.after_epoch) {
                    DumpItem::Modified(unsafe { &*ptr.as_ptr().add(idx as usize) })
                } else {
                    DumpItem::Unmodified
                }
            }
        }
    }
}

macro_rules! impl_dump_query {
    () => {
        /* Don't implement for empty tuple */
    };
    ($($a:ident)*) => {
        impl<$($a),*> AsQuery for DumpQuery<($($a,)*)>
        where
            $($a: 'static,)*
        {
            type Query = Self;
        }

        impl<$($a),*> IntoQuery for DumpQuery<($($a,)*)>
        where
            $($a: 'static,)*
        {
            fn into_query(self) -> Self {
                self
            }
        }

        #[allow(unused_parens)]
        #[allow(non_snake_case)]
        unsafe impl<$($a),*> Query for DumpQuery<($($a,)*)>
        where
            $($a: 'static,)*
        {
            type Item<'a> = (EntityLoc<'a>, ($(DumpItem<'a, $a>),*));
            type Fetch<'a> = (EntitiesFetch<'a>, ($(DumpFetch<'a, $a>),*));

            const MUTABLE: bool = false;
            const FILTERS_ENTITIES: bool = true;

            #[inline(always)]
            fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
                match comp.id() {
                    $(id if id == type_id::<$a>() => Ok(Some(Access::Read)),)*
                    _ => Ok(None),
                }
            }

            #[inline(always)]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                false $(|| archetype.has_component(type_id::<$a>()))*
            }

            #[inline(always)]
            unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
                $(
                    if archetype.has_component(type_id::<$a>()) {
                        f(type_id::<$a>(), Access::Read);
                    }
                )*
            }

            #[inline(always)]
            unsafe fn fetch<'a>(
                &self,
                arch_idx: u32,
                archetype: &'a Archetype,
                epoch: EpochId,
            ) -> (EntitiesFetch<'a>, ($(DumpFetch<'a, $a>),*)) {
                let ($($a,)*) = ($(
                    match archetype.component(type_id::<$a>()) {
                        None => DumpFetch {
                            after_epoch: self.after_epoch,
                            ptr: None,
                            entity_epochs: NonNull::dangling(),
                            marker: PhantomData,
                        },
                        Some(component) => {
                            let data = unsafe { component.data() };
                            DumpFetch {
                                after_epoch: self.after_epoch,
                                ptr: Some(data.ptr.cast()),
                                entity_epochs: unsafe { NonNull::new_unchecked(data.entity_epochs.as_ptr() as *mut EpochId) },
                                marker: PhantomData,
                            }
                        }
                    },)*);
                (unsafe { Entities.fetch(arch_idx, archetype, epoch) }, ($($a),*))
            }
        }

        unsafe impl<$($a),*> ImmutableQuery for DumpQuery<($($a,)*)>
        where
            $($a: 'static,)*
        {}

        #[allow(unused_parens)]
        #[allow(non_snake_case)]
        unsafe impl<$($a),*> SendQuery for DumpQuery<($($a,)*)>
        where
            $($a: 'static,)*
        {
        }
    };
}

for_tuple!(impl_dump_query);
