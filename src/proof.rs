/// Special kind of query to skip components in `World::get/get_mut`.
///
/// Does nothing for all other methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skip;

/// Trait implemented for proofs of pinned components on `Entity`.
///
/// See `World::get/get_mut` for usage.
pub trait Proof<Q> {}

macro_rules! impl_proof {
    () => {
        impl_proof!(@ A1 A2 A3 A4 A5 A6 A7 A8, B1 B2 B3 B4 B5 B6 B7 B8);
    };

    (! $($a:ident)* , $($b:ident)*) => {
        impl<'a, H $(,$a)* $(,$b)*> Proof<(&'a mut H, $($b,)*)> for &'a mut (H, $($a,)*) where  &'a mut ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(&'a     H, $($b,)*)> for &'a mut (H, $($a,)*) where  &'a mut ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(&'a     H, $($b,)*)> for &'a     (H, $($a,)*) where  &'a     ($($a,)*): Proof<($($b,)*)> {}

        impl<'a, H $(,$a)* $(,$b)*> Proof<(Option<&'a mut H>, $($b,)*)> for &'a mut ($($a,)*) where  &'a mut ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(Option<&'a     H>, $($b,)*)> for &'a mut ($($a,)*) where  &'a mut ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(Option<&'a     H>, $($b,)*)> for &'a     ($($a,)*) where  &'a     ($($a,)*): Proof<($($b,)*)> {}

        impl<'a, H $(,$a)* $(,$b)*> Proof<(     Skip, $($b,)*)> for &'a mut (H, $($a,)*) where  &'a mut ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(     Skip, $($b,)*)> for &'a     (H, $($a,)*) where  &'a     ($($a,)*): Proof<($($b,)*)> {}
    };

    (@ $ah:ident $($at:ident)* , $bh:ident $($bt:ident)*) => {
        // Proove nothing and single opt by anything.
        impl<'a, $ah $(,$at)*> Proof<()> for &'a mut ($ah, $($at,)*) {}
        impl<'a, $ah $(,$at)*> Proof<()> for &'a ($ah, $($at,)*) {}

        impl<'a, H, $ah $(,$at)*> Proof<Option<&'a H>> for &'a mut ($ah, $($at,)*) {}
        impl<'a, H, $ah $(,$at)*> Proof<Option<&'a mut H>> for &'a mut ($ah, $($at,)*) {}
        impl<'a, H, $ah $(,$at)*> Proof<Option<&'a H>> for &'a ($ah, $($at,)*) {}

        impl_proof!(% $ah $($at)* , $bh $($bt)*);
        impl_proof!(@ $($at)* , $($bt)*);
    };

    (@ , ) => {
        // Proove nothing and single opt by anything.
        impl<'a> Proof<()> for &'a mut () {}
        impl<'a> Proof<()> for &'a () {}

        impl<'a, H> Proof<Option<&'a H>> for &'a mut () {}
        impl<'a, H> Proof<Option<&'a mut H>> for &'a mut () {}
        impl<'a, H> Proof<Option<&'a H>> for &'a () {}

        impl_proof!(% , );
    };


    (% $ah:ident $($at:ident)* , $($b:ident)*) => {
        impl_proof!(! $ah $($at)* , $($b)* );
        impl_proof!(% $($at)* , $($b)*);
    };

    (% , $($b:ident)*) => {
        impl_proof!(! , $($b)* );
    };

    (% , ) => {
        impl_proof!(! , );
    };
}

impl_proof!();
