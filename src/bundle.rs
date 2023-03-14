//! This module defines [`Bundle`], [`ComponentBundle`], [`DynamicBundle`] and [`DynamicComponentBundle`] traits.
//!
//! Tuples of up to 26 elements implement [`Bundle`] and [`DynamicBundle`] if all elements are `'static`.
//! They additionally implement [`ComponentBundle`] and [`DynamicComponentBundle`] if all elements implement [`Component`].
//!
//! Bundles can be used to spawn entities with a set of components or insert multiple components at once.
//! This is more efficient than spawning an entity and then inserting components one by one.

use core::{
    alloc::Layout,
    any::TypeId,
    fmt,
    marker::PhantomData,
    mem::{align_of, replace, size_of, ManuallyDrop},
    ptr::{self, NonNull},
};

use smallvec::SmallVec;

use crate::component::{Component, ComponentInfo};

/// Possibly dynamic collection of components that may be inserted into the `World`.
///
/// # Safety
///
/// Implementors must uphold requirements:
/// Bundle instance must have a set components.
/// [`DynamicBundle::valid`] must return true only if components are not repeated.
/// [`DynamicBundle::key`] must return unique value for a set of components or `None`
/// [`DynamicBundle::contains_id`] must return true if component type with specified id is contained in bundle.
/// [`DynamicBundle::with_ids`] must call provided function with a list of type ids of all contained components.
/// [`DynamicBundle::put`] must call provided function for each component with pointer to component value, its type id and size.
pub unsafe trait DynamicBundle {
    /// Returns `true` if given bundle is valid.
    fn valid(&self) -> bool;

    /// Returns static key if the bundle type have one.
    fn key() -> Option<TypeId> {
        None
    }

    /// Returns true if bundle has specified type id.
    fn contains_id(&self, id: TypeId) -> bool;

    /// Calls provided closure with slice of ids of types that this bundle contains.
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R;

    /// Calls provided closure with pointer to a component, its type and size.
    /// Closure is expected to read components from the pointer and take ownership.
    fn put(self, f: impl FnMut(NonNull<u8>, TypeId, usize));
}

/// Possibly dynamic collection of components that may be inserted into the `World`.
/// Where all elements implement `Component` and so support auto-registration.
///
/// # Safety
///
/// [`DynamicComponentBundle::with_components`] must call provided function with a list of component infos of all contained components.
pub unsafe trait DynamicComponentBundle: DynamicBundle {
    /// Calls provided closure with slice of component infos of types that this bundle contains.
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R;
}

/// Static collection of components that may be inserted into the `World`.
///
/// # Safety
///
/// Implementors must uphold requirements:
/// Bundle instance must have a set components.
/// [`Bundle::static_valid`] must return true only if components are not repeated.
/// [`Bundle::static_key`] must return unique value for a set of components.
/// [`Bundle::static_contains_id`] must return true if component type with specified id is contained in bundle.
/// [`Bundle::static_with_ids`] must call provided function with a list of type ids of all contained components.

pub unsafe trait Bundle: DynamicBundle {
    /// Returns `true` if given bundle is valid.
    fn static_valid() -> bool;

    /// Returns static key for the bundle type.
    fn static_key() -> TypeId;

    /// Returns true if bundle has specified type id.
    fn static_contains_id(id: TypeId) -> bool;

    /// Calls provided closure with slice of ids of types that this bundle contains.
    fn static_with_ids<R>(f: impl FnOnce(&[TypeId]) -> R) -> R;
}

/// Static collection of components that may be inserted into the `World`.
/// Where all elements implement `Component` and so support auto-registration.
///
/// # Safety
///
/// [`ComponentBundle::static_with_components`] must call provided function with a list of component infos of all contained components.
pub unsafe trait ComponentBundle: Bundle + DynamicComponentBundle {
    /// Calls provided closure with slice of component infos of types that this bundle contains.
    fn static_with_components<R>(f: impl FnOnce(&[ComponentInfo]) -> R) -> R;
}

macro_rules! impl_bundle {
    () => {
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
            fn put(self, _f: impl FnMut(NonNull<u8>, TypeId, usize)) {}
        }

        unsafe impl DynamicComponentBundle for () {
            #[inline]
            fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                Self::static_with_components(f)
            }
        }

        unsafe impl Bundle for () {
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
        }

        unsafe impl ComponentBundle for () {
            #[inline]
            fn static_with_components<R>(f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                f(&[])
            }
        }
    };

    ($($a:ident)+) => {
        unsafe impl<$($a),+> DynamicBundle for ($($a,)+)
        where $($a: 'static,)+
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
            fn put(self, mut f: impl FnMut(NonNull<u8>, TypeId, usize)) {
                #![allow(non_snake_case)]

                let ($($a,)+) = self;
                let ($($a,)+) = ($(ManuallyDrop::new($a),)+);
                $(
                    f(NonNull::from(&*$a).cast(), TypeId::of::<$a>(), size_of::<$a>());
                )+
            }
        }

        unsafe impl<$($a),+> DynamicComponentBundle for ($($a,)+)
        where $($a: Component,)+
        {
            #[inline]
            fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                <Self as ComponentBundle>::static_with_components(f)
            }
        }

        unsafe impl<$($a),+> Bundle for ($($a,)+)
        where $($a: 'static,)+
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
        }


        unsafe impl<$($a),+> ComponentBundle for ($($a,)+)
        where $($a: Component,)+
        {
            #[inline]
            fn static_with_components<R>(f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
                f(&[$(ComponentInfo::of::<$a>(),)+])
            }
        }
    };
}

for_tuple!(impl_bundle);

/// Build entities when exact set of components is not known at compile time.
///
/// Components can be added to [`EntityBuilder`] at runtime using [`EntityBuilder::add`] or [`EntityBuilder::with`].
/// [`EntityBuilder`] then can be used to insert components into entity or spawn a new entity.
pub struct EntityBuilder {
    ptr: NonNull<u8>,
    layout: Layout,
    len: usize,

    ids: SmallVec<[TypeId; 8]>,
    infos: SmallVec<[ComponentInfo; 8]>,
    offsets: SmallVec<[usize; 8]>,
}

// # Safety
// Stores only `Send` values.
unsafe impl Send for EntityBuilder {}

impl Drop for EntityBuilder {
    fn drop(&mut self) {
        for (info, &offset) in self.infos.iter().zip(&self.offsets) {
            let ptr = unsafe { NonNull::new_unchecked(self.ptr.as_ptr().add(offset)) };
            info.final_drop(ptr, 1);
        }
    }
}

impl fmt::Debug for EntityBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut ds = f.debug_struct("EntityBuilder");
        for info in &self.infos {
            ds.field("component", &info.name());
        }
        ds.finish()
    }
}

impl EntityBuilder {
    /// Creates new empty entity builder.
    #[inline]
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
    #[inline]
    pub fn with<T>(mut self, value: T) -> Self
    where
        T: Component + Send,
    {
        self.add(value);
        self
    }

    /// Adds component to the builder.
    /// If builder already had this component, old value is replaced.
    pub fn add<T>(&mut self, value: T) -> &mut Self
    where
        T: Component + Send,
    {
        if let Some(existing) = self.get_mut::<T>() {
            // Replace existing value.
            *existing = value;
            return self;
        }

        debug_assert!(self.len <= self.layout.size());
        let value_layout = Layout::from_size_align(self.len, self.layout.align()).unwrap();

        let (new_value_layout, value_offset) = value_layout
            .extend(Layout::new::<T>())
            .expect("EntityBuilder overflow");

        self.ids.reserve(1);
        self.infos.reserve(1);
        self.offsets.reserve(1);

        if self.layout.align() != new_value_layout.align()
            || self.layout.size() < new_value_layout.size()
        {
            // Those thresholds helps avoiding reallocation.
            const MIN_LAYOUT_ALIGN: usize = align_of::<u128>();
            const MIN_LAYOUT_SIZE: usize = 128;

            let cap = if self.layout.size() < new_value_layout.size() {
                if MIN_LAYOUT_SIZE >= new_value_layout.size() {
                    MIN_LAYOUT_SIZE
                } else {
                    match self.layout.size().checked_mul(2) {
                        Some(cap) if cap >= new_value_layout.size() => cap,
                        _ => new_value_layout.size(),
                    }
                }
            } else {
                self.layout.size()
            };

            let align = new_value_layout.align().max(MIN_LAYOUT_ALIGN);
            let new_layout = Layout::from_size_align(cap, align).unwrap_or(new_value_layout);

            unsafe {
                let new_ptr = alloc::alloc::alloc(new_layout);
                let new_ptr = NonNull::new(new_ptr).unwrap();

                ptr::copy_nonoverlapping(self.ptr.as_ptr(), new_ptr.as_ptr(), self.len);

                let old_ptr = replace(&mut self.ptr, new_ptr);
                let old_layout = replace(&mut self.layout, new_layout);

                alloc::alloc::dealloc(old_ptr.as_ptr(), old_layout);
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

        self
    }

    /// Returns reference to component from builder.
    #[inline]
    pub fn get<T>(&self) -> Option<&T>
    where
        T: 'static,
    {
        let idx = self.ids.iter().position(|id| *id == TypeId::of::<T>())?;
        let offset = self.offsets[idx];
        Some(unsafe { &*self.ptr.as_ptr().add(offset).cast::<T>() })
    }

    /// Returns mutable reference to component from builder.
    #[inline]
    pub fn get_mut<T>(&mut self) -> Option<&mut T>
    where
        T: 'static,
    {
        let idx = self.ids.iter().position(|id| *id == TypeId::of::<T>())?;
        let offset = self.offsets[idx];
        Some(unsafe { &mut *self.ptr.as_ptr().add(offset).cast::<T>() })
    }

    /// Returns iterator over component types in this builder.
    #[inline]
    pub fn component_types(&self) -> impl Iterator<Item = &ComponentInfo> {
        self.infos.iter()
    }

    /// Returns true of the builder is empty.
    #[inline]
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
    fn put(self, mut f: impl FnMut(NonNull<u8>, TypeId, usize)) {
        let me = ManuallyDrop::new(self);
        for (info, &offset) in me.infos.iter().zip(&me.offsets) {
            let ptr = unsafe { NonNull::new_unchecked(me.ptr.as_ptr().add(offset)) };
            f(ptr, info.id(), info.layout().size());
        }
    }
}

unsafe impl DynamicComponentBundle for EntityBuilder {
    #[inline]
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
        f(&self.infos)
    }
}

/// Umbrella trait for [`DynamicBundle`] and [`Bundle`].
pub(super) trait BundleDesc {
    /// Returns static key if the bundle type have one.
    fn key() -> Option<TypeId>;

    /// Calls provided closure with slice of ids of types that this bundle contains.
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R;
}

/// Umbrella trait for [`DynamicBundle`] and [`Bundle`].
pub(super) trait ComponentBundleDesc: BundleDesc {
    /// Calls provided closure with slice of component types that this bundle contains.
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R;
}

impl<B> BundleDesc for B
where
    B: DynamicBundle,
{
    #[inline]
    fn key() -> Option<TypeId> {
        <B as DynamicBundle>::key()
    }

    #[inline]
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
        DynamicBundle::with_ids(self, f)
    }
}

impl<B> ComponentBundleDesc for B
where
    B: DynamicComponentBundle,
{
    #[inline]
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
        DynamicComponentBundle::with_components(self, f)
    }
}

impl<B> BundleDesc for PhantomData<B>
where
    B: Bundle,
{
    #[inline]
    fn key() -> Option<TypeId> {
        Some(B::static_key())
    }

    #[inline]
    fn with_ids<R>(&self, f: impl FnOnce(&[TypeId]) -> R) -> R {
        B::static_with_ids(f)
    }
}

impl<B> ComponentBundleDesc for PhantomData<B>
where
    B: ComponentBundle,
{
    #[inline]
    fn with_components<R>(&self, f: impl FnOnce(&[ComponentInfo]) -> R) -> R {
        B::static_with_components(f)
    }
}
