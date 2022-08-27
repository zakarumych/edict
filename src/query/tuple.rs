use core::any::TypeId;

use crate::{archetype::Archetype, epoch::EpochId};

use super::{fetch::Fetch, merge_access, Access, ImmutableQuery, IntoQuery, Query, QueryFetch};

macro_rules! for_tuple {
    () => {
        for_tuple!(for A B C D E F G H I J K L M N O P Q R S T U V W X Y Z);
        // for_tuple!(for A);
    };

    (for) => {
        for_tuple!(impl);
    };

    (for $head:ident $($tail:ident)*) => {
        for_tuple!(for $($tail)*);
        for_tuple!(impl $head $($tail)*);
    };

    (impl) => {
        unsafe impl Fetch<'_> for () {
            type Item = ();

            #[inline]
            fn dangling() {}

            #[inline]
            unsafe fn skip_chunk(&mut self, _: usize) -> bool {
                false
            }

            #[inline]
            unsafe fn skip_item(&mut self, _: usize) -> bool {
                false
            }

            #[inline]
            unsafe fn visit_chunk(&mut self, _: usize) {}

            #[inline]
            unsafe fn get_item(&mut self, _: usize) {}
        }

        impl QueryFetch<'_> for () {
            type Item = ();
            type Fetch = ();
        }

        impl IntoQuery for () {
            type Query = ();
        }

        impl Query for () {
            #[inline]
            fn access(&self, _ty: TypeId) -> Option<Access> {
                None
            }

            #[inline]
            fn skip_archetype(&self, _: &Archetype) -> bool {
                false
            }

            #[inline]
            unsafe fn fetch(&mut self, _: &Archetype, _: EpochId) -> () {
                ()
            }
        }

        unsafe impl ImmutableQuery for () {}
    };

    (impl $($a:ident)+) => {
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
            unsafe fn skip_chunk(&mut self, chunk_idx: usize) -> bool {
                let ($($a,)+) = self;
                $($a.skip_chunk(chunk_idx) ||)+ false
            }

            /// Checks if item with specified index must be skipped.
            #[inline]
            unsafe fn skip_item(&mut self, idx: usize) -> bool {
                let ($($a,)+) = self;
                $($a.skip_item(idx) ||)+ false
            }

            /// Notifies this fetch that it visits a chunk.
            #[inline]
            unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
                let ($($a,)+) = self;
                $($a.visit_chunk(chunk_idx);)+
            }

            #[inline]
            unsafe fn get_item(&mut self, idx: usize) -> ($($a::Item),+) {
                let ($($a,)+) = self;
                ($( $a.get_item(idx) ),+)
            }
        }

        #[allow(unused_parens)]
        impl<'a $(, $a)+> QueryFetch<'a> for ($($a,)+) where $($a: Query,)+ {
            type Item = ($(<$a as QueryFetch<'a>>::Item),+);
            type Fetch = ($(<$a as QueryFetch<'a>>::Fetch),+);
        }

        #[allow(non_snake_case)]
        #[allow(unused_parens)]
        impl<$($a),+> Query for ($($a,)+) where $($a: Query,)+ {
            #[inline]
            fn access(&self, ty: TypeId) -> Option<Access> {
                let ($($a,)+) = self;
                let mut access = None;
                $(access = merge_access(access, <$a as Query>::access($a, ty));)+
                access
            }

            #[inline]
            fn skip_archetype(&self, archetype: &Archetype) -> bool {
                let ($($a,)+) = self;
                $( <$a as Query>::skip_archetype($a, archetype) )||+
            }

            #[inline]
            #[allow(unused_parens)]
            unsafe fn fetch<'a>(&mut self, archetype: &'a Archetype, epoch: EpochId) -> ($(<$a as QueryFetch<'a>>::Fetch),+) {
                let ($($a,)+) = self;
                ($( <$a as Query>::fetch($a, archetype, epoch) ),+)
            }
        }

        unsafe impl<$($a),+> ImmutableQuery for ($($a,)+) where $($a: ImmutableQuery,)+ {}

        impl<$($a),+> IntoQuery for ($($a,)+) where $($a: IntoQuery,)+ {
            type Query = ($($a::Query,)+);
        }
    };
}

for_tuple!();
