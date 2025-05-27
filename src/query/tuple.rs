use core::any::TypeId;

use crate::{
    archetype::Archetype, component::ComponentInfo, entity::EntityId, epoch::EpochId,
    system::QueryArg, world::World,
};

use super::{
    fetch::{BatchFetch, Fetch},
    Access, AsQuery, DefaultQuery, ImmutableQuery, IntoQuery, Query, SendQuery, WriteAlias,
};

macro_rules! impl_fetch {
    () => {
        unsafe impl Fetch<'_> for () {
            type Item = ();

            #[inline]
            fn dangling() {}

            #[inline]
            unsafe fn get_item(&mut self, _: u32) {}
        }

        unsafe impl BatchFetch<'_> for () {
            type Batch = ();

            #[inline]
            unsafe fn get_batch(&mut self, _: u32, _: u32) {}
        }

        impl AsQuery for () {
            type Query = ();
        }

        impl IntoQuery for () {
            fn into_query(self) -> () {
                ()
            }
        }

        impl DefaultQuery for () {
            #[inline]
            fn default_query() -> () {
                ()
            }
        }

        impl QueryArg for () {
            #[inline]
            fn new() -> () {
                ()
            }
        }

        unsafe impl Query for () {
            type Item<'a> = ();
            type Fetch<'a> = ();

            const MUTABLE: bool = false;

            #[inline]
            fn component_access(&self, _comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
                Ok(None)
            }

            #[inline]
            fn visit_archetype(&self, _: &Archetype) -> bool {
                true
            }

            #[inline]
            unsafe fn access_archetype(&self, _: &Archetype, _: impl FnMut(TypeId, Access)) {}

            #[inline]
            unsafe fn fetch(&self, _: u32, _: &Archetype, _: EpochId) -> () {
                ()
            }

            #[inline]
            fn reserved_entity_item<'a>(&self, _: EntityId, _: u32) -> Option<()> where (): 'a {
                Some(())
            }
        }

        unsafe impl ImmutableQuery for () {}
        unsafe impl SendQuery for () {}
    };

    ($($a:ident)+) => {
        #[allow(unused_parens)]
        #[allow(non_snake_case)]
        unsafe impl<'a $(, $a)+> Fetch<'a> for ($($a,)+)
        where $($a: Fetch<'a>,)+
        {
            type Item = ($($a::Item),+);

            #[inline]
            fn dangling() -> Self {
                ($($a::dangling(),)+)
            }

            #[inline]
            unsafe fn visit_chunk(&mut self, chunk_idx: u32) -> bool {
                let ($($a,)+) = self;
                unsafe { $($a.visit_chunk(chunk_idx) &&)+ true }
            }

            /// Checks if item with specified index must be visited or skipped.
            #[inline]
            unsafe fn visit_item(&mut self, idx: u32) -> bool {
                let ($($a,)+) = self;
                unsafe { $($a.visit_item(idx) &&)+ true }
            }

            /// Notifies this fetch that it visits a chunk.
            #[inline]
            unsafe fn touch_chunk(&mut self, chunk_idx: u32) {
                let ($($a,)+) = self;
                $(unsafe { $a.touch_chunk(chunk_idx); })+
            }

            #[inline]
            unsafe fn get_item(&mut self, idx: u32) -> ($($a::Item),+) {
                let ($($a,)+) = self;
                unsafe {($( $a.get_item(idx) ),+) }
            }
        }

        #[allow(unused_parens)]
        #[allow(non_snake_case)]
        unsafe impl<'a $(, $a)+> BatchFetch<'a> for ($($a,)+)
        where $($a: BatchFetch<'a>,)+
        {
            type Batch = ($($a::Batch),+);

            #[inline]
            unsafe fn get_batch(&mut self, start: u32, end: u32) -> ($($a::Batch),+) {
                let ($($a,)+) = self;
                unsafe {($( $a.get_batch(start, end) ),+) }
            }
        }

        #[allow(non_snake_case)]
        impl<$($a),+> AsQuery for ($($a,)+) where $($a: AsQuery,)+ {
            type Query = ($($a::Query,)+);
        }

        #[allow(non_snake_case)]
        impl<$($a),+> IntoQuery for ($($a,)+) where $($a: IntoQuery,)+ {
            #[inline]
            fn into_query(self) -> Self::Query {
                let ($($a,)+) = self;
                ($( $a.into_query(), )+)
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        impl<$($a),+> DefaultQuery for ($($a,)+) where $($a: DefaultQuery,)+ {
            #[inline]
            fn default_query() -> ($($a::Query,)+) {
                ($($a::default_query(),)+)
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        impl<$($a),+> QueryArg for ($($a,)+) where $($a: QueryArg,)+ {
            #[inline]
            fn new() -> ($($a::Query,)+) {
                ($($a::new(),)+)
            }

            #[inline]
            fn before(&mut self, world: &World) {
                let ($($a,)*) = self;
                $($a.before(world);)*
            }

            #[inline]
            fn after(&mut self, world: &World) {
                let ($($a,)*) = self;
                $($a.after(world);)*
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        unsafe impl<$($a),+> Query for ($($a,)+) where $($a: Query,)+ {
            type Item<'a> = ($($a::Item<'a>),+) where $($a: 'a,)+;
            type Fetch<'a> = ($($a::Fetch<'a>),+) where $($a: 'a,)+;

            const MUTABLE: bool = $($a::MUTABLE ||)+ false;
            const FILTERS_ENTITIES: bool = $($a::FILTERS_ENTITIES ||)+ false;

            #[inline]
            fn component_access(&self, comp: &ComponentInfo) -> Result<Option<Access>, WriteAlias> {
                let ($($a,)+) = self;
                let mut result = None;
                $(
                    result = match (result, $a.component_access(comp)?) {
                        (None, one) | (one, None) => one,
                        (Some(Access::Read), Some(Access::Read)) => Some(Access::Read),
                        (Some(Access::Write), Some(_)) | (Some(_), Some(Access::Write)) => return Err(WriteAlias),
                    };
                )*
                Ok(result)
            }

            #[inline]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)+) = self;
                true $( && <$a as Query>::visit_archetype($a, archetype) )+
            }

            #[inline]
            unsafe fn access_archetype(&self, archetype: &Archetype, mut f: impl FnMut(TypeId, Access)) {
                let ($($a,)+) = self;
                $( unsafe { <$a as Query>::access_archetype($a, archetype, &mut f); } )+
            }

            #[inline]
            unsafe fn visit_archetype_late(&self, archetype: &Archetype) -> bool {
                let ($($a,)+) = self;
                true $( && unsafe { <$a as Query>::visit_archetype_late($a, archetype) } )+
            }

            #[inline]
            unsafe fn fetch<'a>(&self, arch_idx: u32, archetype: &'a Archetype, epoch: EpochId) -> ($($a::Fetch<'a>),+) {
                let ($($a,)+) = self;
                unsafe { ($( <$a as Query>::fetch($a, arch_idx, archetype, epoch) ),+) }
            }

            #[inline]
            fn reserved_entity_item<'a>(&self, id: EntityId, idx: u32) -> Option<($($a::Item<'a>),+)> {
                let ($($a,)+) = self;
                $( let $a = $a.reserved_entity_item(id, idx)?; )+
                Some(($($a),+))
            }
        }

        unsafe impl<$($a),+> ImmutableQuery for ($($a,)+) where $($a: ImmutableQuery,)+ {}
        unsafe impl<$($a),+> SendQuery for ($($a,)+) where $($a: SendQuery,)+ {}
    };
}

for_tuple!(impl_fetch);
