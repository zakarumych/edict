#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skip;

pub trait Proof<Q> {}

macro_rules! impl_proof_opts {
    ($($a:ident),* $(,)? | $($b:ident),* $(,)?) => {
        impl_proof_opts!(! $($a),* | $($b),* | C1, C2, C3, C4);
    };

    (! $($a:ident),* | $($b:ident),* | ) => {};

    (! $($a:ident),* | $($b:ident),* | $($c:ident),+ ) => {
        impl<'a $(,$a)* $(,$b: 'a)* $(, $c)+> Proof<($($b,)* $(Option<$c>,)+)> for &'a mut ($($a,)*) where &'a mut ($($a,)*): Proof<($($b,)*)>  {}
        impl<'a $(,$a)* $(,$b: 'a)* $(, $c)*> Proof<($($b,)* $(Option<$c>,)+)> for &'a ($($a,)*)     where &'a ($($a,)*)    : Proof<($($b,)*)>  {}

        impl_proof_opts!(@ $($a),* | $($b),* | $($c),+);
    };

    (@ $($a:ident),* | $($b:ident),* | $head:ident $(,$tail:ident)* ) => {
        impl_proof_opts!(! $($a),* | $($b),* | $($tail),*);
    };
}

macro_rules! impl_proof {
    () => {
        impl_proof!(! A1, A2, A3 | B1, B2, B3);
    };
    (!|) => {
        impl<'a> Proof<()> for &'a mut () {}
        impl<'a> Proof<()> for &'a () {}

        impl<'a, H> Proof<(&'a mut H, )> for &'a mut (H,) {}
        impl<'a, H> Proof<(&'a     H, )> for &'a mut (H,) {}
        impl<'a, H> Proof<(     Skip, )> for &'a mut (H,) {}
        impl<'a, H> Proof<(&'a     H, )> for &'a     (H,) {}
        impl<'a, H> Proof<(     Skip, )> for &'a     (H,) {}

        impl_proof_opts!(|);
    };

    (! $($a:ident),+ | $($b:ident),+) => {
        impl<'a, H $(,$a)+ $(,$b)+> Proof<(&'a mut H, $($b,)+)> for &'a mut (H, $($a,)+) where  &'a mut ($($a,)+): Proof<($($b,)+)> {}
        impl<'a, H $(,$a)+ $(,$b)+> Proof<(&'a     H, $($b,)+)> for &'a mut (H, $($a,)+) where  &'a mut ($($a,)+): Proof<($($b,)+)> {}
        impl<'a, H $(,$a)+ $(,$b)+> Proof<(     Skip, $($b,)+)> for &'a mut (H, $($a,)+) where  &'a mut ($($a,)+): Proof<($($b,)+)> {}
        impl<'a, H $(,$a)+ $(,$b)+> Proof<(&'a     H, $($b,)+)> for &'a     (H, $($a,)+) where  &'a     ($($a,)+): Proof<($($b,)+)> {}
        impl<'a, H $(,$a)+ $(,$b)+> Proof<(     Skip, $($b,)+)> for &'a     (H, $($a,)+) where  &'a     ($($a,)+): Proof<($($b,)+)> {}

        impl_proof_opts!($($a),+ | $($b),+);

        impl_proof!(@ $($a),+ | $($b),+);
    };

    (@ $ah:ident $(,$at:ident)* | $bh:ident $(,$bt:ident)*) => {
        impl_proof!(! $($at),* | $($bt),*);
    };
}

impl_proof!();
