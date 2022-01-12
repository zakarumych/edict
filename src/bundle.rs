use core::{
    any::TypeId,
    mem::{size_of, ManuallyDrop},
    ptr::NonNull,
};

use crate::component::{Component, ComponentInfo};

pub unsafe trait DynamicBundle {
    /// Returns static key if the bundle type have one.
    fn key() -> Option<TypeId> {
        None
    }
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R;
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R;
    fn put(self, f: impl FnMut(NonNull<u8>, TypeId, usize));
}

pub trait Bundle: DynamicBundle {
    fn static_key() -> TypeId;
    fn static_with_ids<R>(f: impl FnOnce(&[TypeId]) -> R) -> R;
    fn static_with_components<R>(f: impl FnOnce(&[ComponentInfo]) -> R) -> R;
}

macro_rules! for_tuple {
    () => {
        for_tuple!(for A B C D E F G);
    };

    (for) => {
        for_tuple!(impl);
    };

    (for $head:ident $($tail:ident)*) => {
        for_tuple!(for $($tail)*);
        for_tuple!(impl $head $($tail)*);
    };

    (impl) => {
        unsafe impl DynamicBundle for () {
            #[inline]
            fn key() -> Option<TypeId> {
                Some(Self::static_key())
            }

            #[inline]
            fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
                Self::static_with_ids(f)
            }
            #[inline]
            fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                Self::static_with_components(f)
            }

            #[inline]
            fn put(self, _f: impl FnMut(NonNull<u8>, TypeId, usize)) {}
        }

        impl Bundle for () {
            #[inline]
            fn static_key() -> TypeId {
                TypeId::of::<()>()
            }

            #[inline]
            fn static_with_ids<R>(f: impl FnOnce(&[TypeId]) -> R) -> R {
                f(&[])
            }

            #[inline]
            fn static_with_components<R>(f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                f(&[])
            }
        }
    };

    (impl $($a:ident)+) => {
        unsafe impl<$($a),+> DynamicBundle for ($($a,)+)
        where $($a: Component,)+
        {
            #[inline]
            fn key() -> Option<TypeId> {
                Some(Self::static_key())
            }

            #[inline]
            fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
                Self::static_with_ids(f)
            }

            #[inline]
            fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                Self::static_with_components(f)
            }

            #[inline]
            fn put(self, mut f: impl FnMut(NonNull<u8>, TypeId, usize)) {
                #![allow(non_snake_case)]

                let ($($a,)+) = self;
                let ($($a,)+) = ($(ManuallyDrop::new($a),)+);
                $(
                    f(NonNull::from(&$a).cast(), TypeId::of::<$a>(), size_of::<$a>());
                )+
            }
        }

        impl<$($a),+> Bundle for ($($a,)+)
        where $($a: Component,)+
        {
            #[inline]
            fn static_key() -> TypeId {
                TypeId::of::<Self>()
            }

            #[inline]
            fn static_with_ids<R>(f: impl FnOnce(&[TypeId]) -> R) -> R {
                f(&[$(TypeId::of::<$a>(),)+])
            }

            #[inline]
            fn static_with_components<R>(f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                f(&[$(ComponentInfo::of::<$a>(),)+])
            }
        }
    };
}

for_tuple!();
