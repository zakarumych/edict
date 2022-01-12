use core::{fmt, marker::PhantomData, ops::Deref};

use crate::{bundle::Bundle, world::World};

use super::{strong::StrongEntity, weak::WeakEntity};

/// Strong reference to an entity.
/// This value can be used to access an entity and keeps the entity alive.
/// On access to a component, if entity doesn't have accessed component,
/// an error is returned.
///
/// This type is generic over list of pinned components.
/// Pinned components cannot be removed and thus they can be accessed with guarantee.
#[derive(Clone, PartialEq, Eq)]
pub struct Entity<T = ()> {
    pub(super) strong: StrongEntity,
    pub(super) marker: PhantomData<fn() -> T>,
}

impl Entity {
    pub(crate) fn with_bundle<B>(self) -> Entity<B>
    where
        B: Bundle,
    {
        Entity {
            strong: self.strong,
            marker: PhantomData,
        }
    }
}

impl<T> fmt::Debug for Entity<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entity")
            .field("gen", &self.gen.get())
            .field("id", &self.id)
            .finish()
    }
}

impl<T> fmt::Display for Entity<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.strong.weak, f)
    }
}

impl<T> Deref for Entity<T> {
    type Target = WeakEntity;

    fn deref(&self) -> &WeakEntity {
        &self.strong.weak
    }
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
        impl Entity<()> {
            pub fn pin<T>(self, world: &mut World) -> Entity<(T,)> {
                drop(world);
                Entity {
                    strong: self.strong,
                    marker: PhantomData,
                }
            }
        }
    };

    (impl $($a:ident)+) => {
        impl<$($a),+> Entity<($($a,)+)> {
            pub fn pin<T>(self, world: &mut World) -> Entity<($($a,)+ T,)> {
                drop(world);
                Entity {
                    strong: self.strong,
                    marker: PhantomData,
                }
            }
        }
    };
}

for_tuple!();
