pub use ::edict_proc::{PinComponent, UnpinComponent};

/// Trait that is implemented for all types that can act as component.
///
pub trait Component: 'static {}

impl<T> Component for T where T: 'static {}

pub trait Archetype {}

macro_rules! impl_archetype {
    () => {
        impl_archetype!(! A, B, C, D, E, F, G, H );
    };

    (!) => {
        impl Archetype for () {}
    };

    (! $($a:ident),+) => {
        impl<$($a),+> Archetype for ($($a,)+) where $($a: Component,)+ {}

        impl_archetype!(@ $($a),*);
    };

    (@ $head:ident $(, $tail:ident)*) => {
        impl_archetype!(! $($tail),*);
    };
}

impl_archetype!();
