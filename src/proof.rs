#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skip;

pub trait Proof<Q> {}

macro_rules! impl_proof {
    () => {
        impl_proof!(@ A1 A2 A3 A4 A5 , B1 B2 B3 B4 B5);
    };

    (! $($a:ident)* , $($b:ident)*) => {
        impl<'a, H $(,$a)* $(,$b)*> Proof<(&'a mut H, $($b,)*)> for &'a mut (H, $($a,)*) where  &'a mut ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(&'a     H, $($b,)*)> for &'a mut (H, $($a,)*) where  &'a mut ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(     Skip, $($b,)*)> for &'a mut (H, $($a,)*) where  &'a mut ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(&'a     H, $($b,)*)> for &'a     (H, $($a,)*) where  &'a     ($($a,)*): Proof<($($b,)*)> {}
        impl<'a, H $(,$a)* $(,$b)*> Proof<(     Skip, $($b,)*)> for &'a     (H, $($a,)*) where  &'a     ($($a,)*): Proof<($($b,)*)> {}
    };

    (@ $ah:ident $($at:ident)* , $bh:ident $($bt:ident)*) => {
        impl_proof!(% $ah $($at)* , $bh $($bt)*);

        impl_proof!(@ $($at)* , $($bt)*);
    };

    (@ , ) => {
        impl_proof!(% , );
    };


    (% $ah:ident $($at:ident)* , $($b:ident)*) => {
        impl_proof!(! $ah $($at)* , $($b)* );
        impl_proof!(% $($at)* , $($b)*);
    };

    (% , $($b:ident)*) => {
        impl_proof!(! , $($b)* );

        impl<'a $(,$b)*> Proof<($(Option<$b>,)*)> for &'a mut () {}
        impl<'a $(,$b)*> Proof<($(Option<$b>,)*)> for &'a     () {}
    };

    (% , ) => {
        impl_proof!(! , );
    };
}

impl_proof!();
