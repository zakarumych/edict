//! This module implements the [`Bundle`] and [`DynanicBundle`] traits,
//! which enables to build entities efficiently.

use core::{
    alloc::Layout,
    any::TypeId,
    fmt,
    mem::{size_of, swap, ManuallyDrop},
    ptr::{self, NonNull},
};

use smallvec::SmallVec;

use crate::component::{Component, ComponentInfo};

/// Possible dynamic collection of components that may be inserted into the `World`.
pub unsafe trait DynamicBundle {
    /// Returns `true` if given bundle is valid.
    fn valid(&self) -> bool;

    /// Returns static key if the bundle type have one.
    fn key() -> Option<TypeId> {
        None
    }

    /// Returns true if bundle has speicifed type id.
    fn contains_id(&self, id: TypeId) -> bool;

    /// Calls provided closure with slice of ids of types that this bundle contains.
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R;

    /// Calls provided closure with slice of component infos of types that this bundle contains.
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R;

    /// Calls provided closure with pointer to a component, its type and size.
    /// Closure is expected to read components from the pointer and take ownership.
    fn put(self, f: impl FnMut(NonNull<u8>, TypeId, usize));
}

/// Static collection of components that may be inserted into the `World`.
pub trait Bundle: DynamicBundle {
    /// Returns `true` if given bundle is valid.
    fn static_valid() -> bool;

    /// Returns static key for the bundle type.
    fn static_key() -> TypeId;

    /// Returns true if bundle has speicifed type id.
    fn static_contains_id(id: TypeId) -> bool;

    /// Calls provided closure with slice of ids of types that this bundle contains.
    fn static_with_ids<R>(f: impl FnOnce(&[TypeId]) -> R) -> R;

    /// Calls provided closure with slice of component infos of types that this bundle contains.
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
            fn valid(&self) -> bool { true }

            #[inline]
            fn key() -> Option<TypeId> {
                Some(Self::static_key())
            }

            #[inline]
            fn contains_id(&self, id: TypeId) -> bool {
                Self::static_contains_id(id)
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
            fn static_valid() -> bool { true }

            #[inline]
            fn static_key() -> TypeId {
                TypeId::of::<()>()
            }

            #[inline]
            fn static_contains_id(_id: TypeId) -> bool {
                false
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
            fn valid(&self) -> bool {
                <Self as Bundle>::static_valid()
            }

            #[inline]
            fn key() -> Option<TypeId> {
                Some(<Self as Bundle>::static_key())
            }

            #[inline]
            fn contains_id(&self, id: TypeId) -> bool {
                <Self as Bundle>::static_contains_id(id)
            }

            #[inline]
            fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
                <Self as Bundle>::static_with_ids(f)
            }

            #[inline]
            fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                <Self as Bundle>::static_with_components(f)
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
            fn static_valid() -> bool {
                let mut ids: &[_] = &[$(TypeId::of::<$a>(),)+];
                while let [check, rest @ ..] = ids {
                    let mut rest = rest;
                    if let [head, tail @ ..] = rest {
                        if head == check {
                            return false;
                        }
                        rest = tail;
                    }
                    ids = rest;
                }
                true
            }

            #[inline]
            fn static_key() -> TypeId {
                TypeId::of::<Self>()
            }

            #[inline]
            fn static_contains_id(id: TypeId) -> bool {
                $( TypeId::of::<$a>() == id )|| *
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

/// Builder for an entity.
/// Entitiy can be spawned with entity builder.
/// See [`World::spawn`] and [`World::spawn_owning`].
pub struct EntityBuilder {
    ptr: NonNull<u8>,
    layout: Layout,
    len: usize,

    ids: SmallVec<[TypeId; 8]>,
    infos: SmallVec<[ComponentInfo; 8]>,
    offsets: SmallVec<[usize; 8]>,
}

impl fmt::Debug for EntityBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("EntityBuilder");
        for info in &self.infos {
            ds.field("component", &info.debug_name);
        }
        ds.finish()
    }
}

impl EntityBuilder {
    /// Creates new empty entity builder.
    pub fn new() -> Self {
        EntityBuilder {
            ptr: NonNull::dangling(),
            len: 0,
            layout: Layout::new::<[u8; 0]>(),
            ids: SmallVec::new(),
            infos: SmallVec::new(),
            offsets: SmallVec::new(),
        }
    }

    /// Adds component to the builder.
    /// If builder already had this component, old value is replaced.
    pub fn add<T>(&mut self, value: T)
    where
        T: Component,
    {
        if let Some(existing) = self.get_mut::<T>() {
            // Replace existing value.
            *existing = value;
            return;
        }

        debug_assert!(self.len <= self.layout.size());
        let layout = Layout::from_size_align(self.len, self.layout.align()).unwrap();

        let (layout, value_offset) = layout
            .extend(Layout::new::<T>())
            .expect("EntityBuilder overflow");

        self.ids.reserve(1);
        self.infos.reserve(1);
        self.offsets.reserve(1);

        if self.layout.align() != layout.align() || self.layout.size() < layout.size() {
            let mut layout = match self.layout.size().checked_mul(2) {
                Some(cap) if cap >= layout.size() => {
                    match Layout::from_size_align(cap, layout.align()) {
                        Err(_) => layout,
                        Ok(layout) => layout,
                    }
                }
                _ => layout,
            };

            unsafe {
                let ptr = alloc::alloc::alloc(layout);
                let mut ptr = NonNull::new(ptr).unwrap();

                ptr::copy_nonoverlapping(self.ptr.as_ptr(), ptr.as_ptr(), self.len);

                swap(&mut self.ptr, &mut ptr);
                swap(&mut self.layout, &mut layout);

                alloc::alloc::dealloc(ptr.as_ptr(), layout);
            }
        }

        unsafe {
            debug_assert!(self.len <= self.layout.size());
            debug_assert!(self.len <= value_offset);
            debug_assert!(value_offset + size_of::<T>() <= self.layout.size());

            ptr::write(self.ptr.as_ptr().add(value_offset).cast(), value);
            self.len = value_offset + size_of::<T>();
        }

        self.ids.push(TypeId::of::<T>());
        self.infos.push(ComponentInfo::of::<T>());
        self.offsets.push(value_offset);
    }

    /// Returns reference to component from builder.
    pub fn get<T>(&self) -> Option<&T>
    where
        T: 'static,
    {
        let idx = self.ids.iter().position(|id| *id == TypeId::of::<T>())?;
        let offset = self.offsets[idx];
        Some(unsafe { &*self.ptr.as_ptr().add(offset).cast::<T>() })
    }

    /// Returns mutable reference to component from builder.
    pub fn get_mut<T>(&mut self) -> Option<&mut T>
    where
        T: 'static,
    {
        let idx = self.ids.iter().position(|id| *id == TypeId::of::<T>())?;
        let offset = self.offsets[idx];
        Some(unsafe { &mut *self.ptr.as_ptr().add(offset).cast::<T>() })
    }

    /// Returns iterator over component types in this builder.
    pub fn component_types(&self) -> impl Iterator<Item = &ComponentInfo> {
        self.infos.iter()
    }

    /// Returns true of the builder is empty.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

unsafe impl DynamicBundle for EntityBuilder {
    #[inline]
    fn valid(&self) -> bool {
        // Validity is ensured by construction
        true
    }

    #[inline]
    fn contains_id(&self, target: TypeId) -> bool {
        self.ids.iter().any(|id| *id == target)
    }

    #[inline]
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
        f(&self.ids)
    }

    #[inline]
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
        f(&self.infos)
    }

    #[inline]
    fn put(self, mut f: impl FnMut(NonNull<u8>, TypeId, usize)) {
        for (info, &offset) in self.infos.iter().zip(&self.offsets) {
            let ptr = unsafe { NonNull::new_unchecked(self.ptr.as_ptr().add(offset)) };
            f(ptr, info.id, info.layout.size());
        }
    }
}
