use crate::{proof::Skip, Component};

pub trait Query {}

pub trait Fetch {}

impl<T> Fetch for &T where T: Component {}
impl<T> Fetch for &mut T where T: Component {}

impl<T> Fetch for Option<T> where T: Fetch {}
impl Fetch for Skip {}

macro_rules! impl_query {
    () => {
        impl_query!(! A, B, C, D, E, F, G, H );
    };

    (!) => {
        impl Query for () {}
    };

    (! $($a:ident),+) => {
        impl<$($a),+> Query for ($($a,)+) where $($a: Fetch,)+ {}

        impl_query!(@ $($a),*);
    };

    (@ $head:ident $(, $tail:ident)*) => {
        impl_query!(! $($tail),*);
    };
}

impl_query!();
