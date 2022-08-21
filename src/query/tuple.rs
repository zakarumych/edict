use core::{any::TypeId, marker::PhantomData};

use crate::archetype::Archetype;

use super::{
    fetch::Fetch,
    merge_access,
    phantom::{ImmutablePhantomQuery, PhantomQuery, PhantomQueryFetch},
    Access, ImmutableQuery, Query, QueryFetch,
};

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
        for_tuple!(for A B C D E F G H I J K L M N O P);
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
            fn skip_archetype_unconditionally(&self, _: &Archetype) -> bool {
                false
            }

            #[inline]
            fn skip_archetype(&self, _: &Archetype) -> bool {
                false
            }

            #[inline]
            unsafe fn fetch(&mut self, _: &Archetype, _: u64) -> () {
                ()
            }
        }

        impl PhantomQueryFetch<'_> for () {
            type Item = ();
            type Fetch = ();
        }

        unsafe impl PhantomQuery for () {

            #[inline]
            fn access(_ty: TypeId) -> Option<Access> {
                None
            }

            #[inline]
            fn access_any() -> Option<Access> {
                None
            }

            #[inline]
            fn conflicts<Q>(_: &Q) -> bool
            where
                Q: Query,
            {
                false
            }

            #[inline]
            fn is_valid() -> bool {
                true
            }

            #[inline]
            fn skip_archetype_unconditionally(_: &Archetype) -> bool {
                false
            }

            #[inline]
            fn skip_archetype(_: &Archetype) -> bool {
                false
            }

            #[inline]
            unsafe fn fetch(_: &Archetype, _: u64) -> () {
                ()
            }
        }

        unsafe impl ImmutableQuery for () {}
        unsafe impl ImmutablePhantomQuery for () {}
    };

    (impl $($a:ident)+) => {
        #[allow(unused_parens)]
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
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                $($a.skip_chunk(chunk_idx) ||)+ false
            }

            /// Checks if item with specified index must be skipped.
            #[inline]
            unsafe fn skip_item(&mut self, idx: usize) -> bool {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                $($a.skip_item(idx) ||)+ false
            }

            /// Notifies this fetch that it visits a chunk.
            #[inline]
            unsafe fn visit_chunk(&mut self, chunk_idx: usize) {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                $($a.visit_chunk(chunk_idx);)+
            }

            #[inline]
            unsafe fn get_item(&mut self, idx: usize) -> ($($a::Item),+) {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                ($( $a.get_item(idx) ),+)
            }
        }

        #[allow(unused_parens)]
        impl<'a $(, $a)+> QueryFetch<'a> for ($($a,)+) where $($a: Query,)+ {
            type Item = ($(<$a as QueryFetch<'a>>::Item),+);
            type Fetch = ($(<$a as QueryFetch<'a>>::Fetch),+);
        }

        unsafe impl<$($a),+> Query for ($($a,)+) where $($a: Query,)+ {
            #[inline]
            fn access(&self, ty: TypeId) -> Option<Access> {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                let mut access = None;
                $(access = merge_access(access, <$a as Query>::access($a, ty));)+
                access
            }

            #[inline]
            fn access_any(&self) -> Option<Access> {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                let mut access = None;
                $(access = merge_access(access, <$a as Query>::access_any($a));)+
                access
            }

            #[inline]
            fn conflicts<Q>(&self, other: &Q) -> bool
            where
                Q: Query,
            {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                $( <$a as Query>::conflicts::<Q>($a, other) ) || +
            }

            #[inline]
            fn is_valid(&self) -> bool {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                let QueryConflict(conflict, _) = $( QueryConflict(false, $a) ) | +;
                !conflict
            }

            #[inline]
            fn skip_archetype_unconditionally(&self, archetype: &Archetype) -> bool {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                $( <$a as Query>::skip_archetype_unconditionally($a, archetype) )||+
            }

            #[inline]
            fn skip_archetype(&self, archetype: &Archetype) -> bool {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                $( <$a as Query>::skip_archetype($a, archetype) )||+
            }

            #[inline]
            #[allow(unused_parens)]
            unsafe fn fetch<'a>(&mut self, archetype: &'a Archetype, epoch: u64) -> ($(<$a as QueryFetch<'a>>::Fetch),+) {
                #[allow(non_snake_case)]
                let ($($a,)+) = self;
                ($( <$a as Query>::fetch($a, archetype, epoch) ),+)
            }
        }

        unsafe impl<$($a),+> ImmutableQuery for ($($a,)+) where $($a: ImmutableQuery,)+ {}

        #[allow(unused_parens)]
        impl<'a $(, $a)+> PhantomQueryFetch<'a> for ($($a,)+) where $($a: PhantomQuery,)+ {
            type Item = ($(<$a as PhantomQueryFetch<'a>>::Item),+);
            type Fetch = ($(<$a as PhantomQueryFetch<'a>>::Fetch),+);
        }

        unsafe impl<$($a),+> PhantomQuery for ($($a,)+) where $($a: PhantomQuery,)+ {

            #[inline]
            fn access(ty: TypeId) -> Option<Access> {
                let mut access = None;
                $(access = merge_access(access, <$a as PhantomQuery>::access(ty));)+
                access
            }

            #[inline]
            fn access_any() -> Option<Access> {
                let mut access = None;
                $(access = merge_access(access, <$a as PhantomQuery>::access_any());)+
                access
            }

            #[inline]
            fn conflicts<Q>(query: &Q) -> bool
            where
                Q: Query,
            {
                $( <$a as PhantomQuery>::conflicts::<Q>(query) ) || +
            }

            #[inline]
            fn is_valid() -> bool {
                let QueryConflict(conflict, _) = $( QueryConflict(false, &PhantomData::<$a>) ) | +;
                !conflict
            }

            #[inline]
            fn skip_archetype_unconditionally(archetype: &Archetype) -> bool {
                $( <$a as PhantomQuery>::skip_archetype_unconditionally(archetype) )||+
            }

            #[inline]
            fn skip_archetype(archetype: &Archetype) -> bool {
                $( <$a as PhantomQuery>::skip_archetype(archetype) )||+
            }

            #[inline]
            #[allow(unused_parens)]
            unsafe fn fetch<'a>(archetype: &'a Archetype, epoch: u64) -> ($(<$a as PhantomQueryFetch<'a>>::Fetch),+) {
                ($( <$a as PhantomQuery>::fetch(archetype, epoch) ),+)
            }
        }

        unsafe impl<$($a),+> ImmutablePhantomQuery for ($($a,)+) where $($a: ImmutablePhantomQuery,)+ {}
    };
}

for_tuple!();
