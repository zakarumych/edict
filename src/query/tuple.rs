use core::any::TypeId;

use crate::{archetype::Archetype, epoch::EpochId};

use super::{fetch::Fetch, merge_access, Access, ImmutableQuery, IntoQuery, Query, QueryFetch};

struct QueryConflict<Q>(bool, Q);

trait Conflicts<C> {
    fn conflicts(&self, candidate: &C) -> bool;
}

impl<T, C> Conflicts<C> for &T
where
    T: Query,
    C: Query,
{
    fn conflicts(&self, candidate: &C) -> bool {
        Query::conflicts(*self, candidate)
    }
}

impl<A, B, C> Conflicts<C> for (A, B)
where
    A: Conflicts<C>,
    B: Conflicts<C>,
    C: Query,
{
    fn conflicts(&self, candidate: &C) -> bool {
        self.0.conflicts(candidate) || self.1.conflicts(candidate)
    }
}

impl<'a, Q, C> core::ops::BitOr<QueryConflict<&'a Q>> for QueryConflict<C>
where
    Q: Query,
    C: Conflicts<Q>,
{
    type Output = QueryConflict<(C, &'a Q)>;

    #[inline]
    fn bitor(self, rhs: QueryConflict<&'a Q>) -> QueryConflict<(C, &'a Q)> {
        let conflict = self.0 || rhs.0 || self.1.conflicts(rhs.1);
        QueryConflict(conflict, (self.1, rhs.1))
    }
}

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

        unsafe impl Query for () {
            #[inline]
            fn access(&self, _ty: TypeId) -> Option<Access> {
                None
            }

            #[inline]
            fn access_any(&self) -> Option<Access> {
                None
            }

            #[inline]
            fn conflicts<Q>(&self, _: &Q) -> bool
            where
                Q: Query,
            {
                false
            }

            #[inline]
            fn is_valid(&self) -> bool {
                true
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
        unsafe impl<$($a),+> Query for ($($a,)+) where $($a: Query,)+ {
            #[inline]
            fn access(&self, ty: TypeId) -> Option<Access> {
                let ($($a,)+) = self;
                let mut access = None;
                $(access = merge_access(access, <$a as Query>::access($a, ty));)+
                access
            }

            #[inline]
            fn access_any(&self) -> Option<Access> {
                let ($($a,)+) = self;
                let mut access = None;
                $(access = merge_access(access, <$a as Query>::access_any($a));)+
                access
            }

            #[inline]
            fn conflicts<Other>(&self, other: &Other) -> bool
            where
                Other: Query,
            {
                let ($($a,)+) = self;
                $( <$a as Query>::conflicts::<Other>($a, other) ) || +
            }

            #[inline]
            fn is_valid(&self) -> bool {
                let ($($a,)+) = self;
                let QueryConflict(conflict, _) = $( QueryConflict(false, $a) ) | +;
                !conflict
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
