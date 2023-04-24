use core::any::TypeId;

use crate::{archetype::Archetype, entity::EntityId, epoch::EpochId};

use super::{fetch::Fetch, merge_access, Access, DefaultQuery, ImmutableQuery, IntoQuery, Query};

macro_rules! impl_fetch {
    () => {
        unsafe impl Fetch<'_> for () {
            type Item = ();

            #[inline]
            fn dangling() {}

            #[inline]
            unsafe fn get_item(&mut self, _: usize) {}
        }

        impl IntoQuery for () {
            type Query = ();

            fn into_query(self) -> () {
                ()
            }
        }

        unsafe impl Query for () {
            type Item<'a> = ();
            type Fetch<'a> = ();

            #[inline]
            fn access(&self, _ty: TypeId) -> Option<Access> {
                None
            }

            #[inline]
            fn visit_archetype(&self, _: &Archetype) -> bool {
                true
            }

            #[inline]
            unsafe fn access_archetype(&self, _: &Archetype, _: &dyn Fn(TypeId, Access)) {}

            #[inline]
            unsafe fn fetch(&mut self, _: &Archetype, _: EpochId) -> () {
                ()
            }

            #[inline]
            fn reserved_entity_item<'a>(&self, _id: EntityId) -> Option<()> where (): 'a {
                Some(())
            }
        }

        unsafe impl ImmutableQuery for () {}
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
            unsafe fn visit_chunk(&mut self, chunk_idx: usize) -> bool {
                let ($($a,)+) = self;
                $($a.visit_chunk(chunk_idx) &&)+ true
            }

            /// Checks if item with specified index must be visited or skipped.
            #[inline]
            unsafe fn visit_item(&mut self, idx: usize) -> bool {
                let ($($a,)+) = self;
                $($a.visit_item(idx) &&)+ true
            }

            /// Notifies this fetch that it visits a chunk.
            #[inline]
            unsafe fn touch_chunk(&mut self, chunk_idx: usize) {
                let ($($a,)+) = self;
                $($a.touch_chunk(chunk_idx);)+
            }

            #[inline]
            unsafe fn get_item(&mut self, idx: usize) -> ($($a::Item),+) {
                let ($($a,)+) = self;
                ($( $a.get_item(idx) ),+)
            }
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        unsafe impl<$($a),+> Query for ($($a,)+) where $($a: Query,)+ {
            type Item<'a> = ($($a::Item<'a>),+);
            type Fetch<'a> = ($($a::Fetch<'a>),+);

            #[inline]
            fn access(&self, ty: TypeId) -> Option<Access> {
                let ($($a,)+) = self;
                let mut access = None;
                $(access = merge_access(access, <$a as Query>::access($a, ty));)+
                access
            }

            #[inline]
            fn visit_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)+) = self;
                $( <$a as Query>::visit_archetype($a, archetype) )&&+
            }

            #[inline]
            unsafe fn access_archetype(&self, archetype: &Archetype, f: &dyn Fn(TypeId, Access)) {
                let ($($a,)+) = self;
                $( <$a as Query>::access_archetype($a, archetype, f); )+
            }

            #[inline]
            unsafe fn fetch<'a>(&mut self, archetype: &'a Archetype, epoch: EpochId) -> ($($a::Fetch<'a>),+) {
                let ($($a,)+) = self;
                ($( <$a as Query>::fetch($a, archetype, epoch) ),+)
            }

            #[inline]
            fn reserved_entity_item<'a>(&self, id: EntityId) -> Option<($($a::Item<'a>),+)> {
                let ($($a,)+) = self;
                $( let $a = $a.reserved_entity_item(id)?; )+
                Some(($($a),+))
            }
        }

        unsafe impl<$($a),+> ImmutableQuery for ($($a,)+) where $($a: ImmutableQuery,)+ {}

        #[allow(non_snake_case)]
        impl<$($a),+> IntoQuery for ($($a,)+) where $($a: IntoQuery,)+ {
            type Query = ($($a::Query,)+);

            fn into_query(self) -> Self::Query {
                let ($($a,)+) = self;
                ($( $a.into_query(), )+)
            }
        }

        #[allow(non_snake_case)]
        impl<$($a),+> DefaultQuery for ($($a,)+) where $($a: DefaultQuery,)+ {
            fn default_query() -> Self::Query {
                ($( $a::default_query(), )+)
            }
        }
    };
}

for_tuple!(impl_fetch);
