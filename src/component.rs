//! This module implements [`Component`] trait for all suitable types.

use core::{
    alloc::Layout,
    any::{type_name, TypeId},
    borrow::{Borrow, BorrowMut},
    marker::PhantomData,
    mem::{transmute, ManuallyDrop},
    ptr::{self, drop_in_place, slice_from_raw_parts_mut, NonNull},
};

use alloc::sync::Arc;
use hashbrown::hash_map::HashMap;

use crate::{action::ActionEncoder, any::UnsafeAny, entity::EntityId, hash::NoOpHasherBuilder};

#[doc(hidden)]
pub type BorrowFn<T> = for<'r> unsafe fn(NonNull<u8>, PhantomData<&'r ()>) -> &'r T;

#[doc(hidden)]
pub type BorrowFnMut<T> = for<'r> unsafe fn(NonNull<u8>, PhantomData<&'r mut ()>) -> &'r mut T;

/// Allows borrows from component by queries.
#[derive(Clone, Copy)]
#[allow(missing_debug_implementations)]
#[allow(missing_docs)]
pub struct ComponentBorrow {
    target: TypeId,

    // Actually is `BorrowFn<T>` where `TypeId::of::<T>() == target`.
    borrow: BorrowFn<()>,

    // Actually is `BorrowFnMut<T>` where `TypeId::of::<T>() == target`.
    borrow_mut: Option<BorrowFnMut<()>>,
}

/// Constructs `ComponentBorrow` to borrow dyn trait object.
#[macro_export]
macro_rules! borrow_dyn_trait {
    ($self:ident as $trait:path) => {{
        fn make_borrow<T>() -> $crate::component::ComponentBorrow
        where
            T: $crate::component::Component + $trait,
        {
            $crate::component::ComponentBorrow::make(
                |ptr: core::ptr::NonNull<u8>, core::marker::PhantomData| -> &dyn $trait {
                    unsafe { ptr.cast::<T>().as_ref() }
                },
                core::option::Option::Some(
                    |ptr: core::ptr::NonNull<u8>, core::marker::PhantomData| -> &mut dyn $trait {
                        unsafe { ptr.cast::<T>().as_mut() }
                    },
                ),
            )
        }

        make_borrow::<$self>()
    }};
}

/// Constructs `ComponentBorrow` to borrow dyn trait object.
#[macro_export]
macro_rules! borrow_dyn_any {
    ($self:ident) => {{
        $crate::borrow_dyn_trait!($self as core::any::Any)
    }};
}

impl ComponentBorrow {
    /// Constructs `ComponentBorrow` from its fields.
    /// Not public API. intended to be used by macros only.
    #[doc(hidden)]
    pub fn make<T: ?Sized + 'static>(
        borrow: BorrowFn<T>,
        borrow_mut: Option<BorrowFnMut<T>>,
    ) -> Self {
        ComponentBorrow {
            target: TypeId::of::<T>(),
            borrow: unsafe { transmute(borrow) },
            borrow_mut: borrow_mut.map(|f| unsafe { transmute(f) }),
        }
    }

    /// Creates new `ComponentBorrow` from type to borrow
    /// using `Borrow` impl.
    pub fn from_borrow<T, U>() -> Self
    where
        T: Component + Borrow<U>,
        U: ?Sized + 'static,
    {
        ComponentBorrow::make(|ptr, PhantomData| unsafe { ptr.cast::<T>().as_ref() }, None)
    }

    /// Creates new `ComponentBorrow` from type to borrow
    /// using `BorrowMut` impl.
    pub fn from_borrow_mut<T, U>() -> Self
    where
        T: Component + BorrowMut<U>,
        U: ?Sized + 'static,
    {
        ComponentBorrow::make(
            |ptr, PhantomData| unsafe { ptr.cast::<T>().as_ref() },
            Some(|ptr, PhantomData| unsafe { ptr.cast::<T>().as_mut() }),
        )
    }

    /// Returns type to borrow self.
    pub fn auto<T>() -> Self
    where
        T: Component,
    {
        ComponentBorrow::make(
            |ptr, PhantomData| unsafe { ptr.cast::<T>().as_ref() },
            Some(|ptr, PhantomData| unsafe { ptr.cast::<T>().as_mut() }),
        )
    }

    pub(crate) fn target(&self) -> TypeId {
        self.target
    }

    pub(crate) fn borrow<'a, T: ?Sized + 'static>(&self) -> BorrowFn<T> {
        debug_assert!(self.target == TypeId::of::<T>());
        unsafe { transmute(self.borrow) }
    }

    pub(crate) fn has_borrow_mut(&self) -> bool {
        self.borrow_mut.is_some()
    }

    pub(crate) fn borrow_mut<'a, T: ?Sized + 'static>(&self) -> Option<BorrowFnMut<T>> {
        debug_assert!(self.target == TypeId::of::<T>());
        unsafe { self.borrow_mut.map(|f| transmute(f)) }
    }
}

/// Trait that is implemented for all types that can act as a component.
/// Currently is implemented for all `'static` types.
pub trait Component: Sized + 'static {
    /// Hook that is executed when entity with component is dropped.
    #[inline]
    fn on_drop(&mut self, entity: EntityId, encoder: &mut ActionEncoder) {
        drop(entity);
        drop(encoder);
    }

    /// Hook that is executed whenever new value is assigned to the component.
    /// If this method returns `true` then `on_remove` is executed for old value before assignment.
    #[inline]
    fn on_set(&mut self, value: &Self, entity: EntityId, encoder: &mut ActionEncoder) -> bool {
        drop(value);
        drop(entity);
        drop(encoder);
        true
    }

    /// Returns array of component borrows supported by the type.
    #[inline]
    fn borrows<F, R>(f: F) -> R
    where
        F: FnOnce(&[ComponentBorrow]) -> R,
    {
        f(&[ComponentBorrow::auto::<Self>()])
    }
}

/// Type information required for components.
#[derive(Clone)]
#[allow(missing_debug_implementations)]
pub struct ComponentInfo {
    /// [`TypeId`] of the component.
    id: TypeId,

    /// [`Layout`] of the component.
    layout: Layout,

    /// [`type_name`] of the component.
    debug_name: &'static str,

    /// Function that calls drop glue for a component.
    /// Supports custom hooks.
    drop_one: DropOneFn,

    /// Context for `drop_one` command when component is dropped.
    on_drop: Arc<UnsafeAny>,

    /// Function that replaces component at target location.
    /// Supports custom hooks.
    set_one: SetOneFn,

    /// Context for `set_one` command.
    on_set: Arc<UnsafeAny>,

    /// Function that calls drop glue for a component.
    /// Does not support custom hooks.
    final_drop: FinalDrop,

    /// An array of possible component borrows.
    borrows: Arc<[ComponentBorrow]>,
}

impl ComponentInfo {
    /// Returns component information for specified component type.
    #[inline(always)]
    pub fn of<T>() -> Self
    where
        T: Component,
    {
        ComponentInfo {
            id: TypeId::of::<T>(),
            layout: Layout::new::<T>(),
            debug_name: type_name::<T>(),
            drop_one: drop_one::<T, DefaultDropHook>,
            on_drop: UnsafeAny::from_arc(Arc::new(DefaultDropHook)),
            set_one: set_one::<T, DefaultSetHook, DefaultDropHook>,
            on_set: UnsafeAny::from_arc(Arc::new(DefaultSetHook)),
            final_drop: final_drop::<T>,
            borrows: T::borrows(|slice| Arc::from(slice)),
        }
    }

    #[inline(always)]
    pub(crate) fn id(&self) -> TypeId {
        self.id
    }

    #[inline(always)]
    pub(crate) fn layout(&self) -> Layout {
        self.layout
    }

    #[inline(always)]
    pub(crate) fn drop_one(&self, ptr: NonNull<u8>, entity: EntityId, encoder: &mut ActionEncoder) {
        unsafe {
            (self.drop_one)(&self.on_drop, ptr, entity, encoder);
        }
    }

    #[inline(always)]
    pub(crate) fn set_one(
        &self,
        dst: NonNull<u8>,
        src: NonNull<u8>,
        entity: EntityId,
        encoder: &mut ActionEncoder,
    ) {
        unsafe {
            (self.set_one)(&self.on_set, &self.on_drop, dst, src, entity, encoder);
        }
    }

    #[inline(always)]
    pub(crate) fn final_drop(&self, ptr: NonNull<u8>, count: usize) {
        unsafe {
            (self.final_drop)(ptr, count);
        }
    }

    #[inline(always)]
    pub(crate) fn debug_name(&self) -> &'static str {
        self.debug_name
    }

    #[inline]
    pub(crate) fn borrows(&self) -> &[ComponentBorrow] {
        &self.borrows
    }
}

/// Trait to be implemented by custom drop hooks.
/// Has blanket implementation for `Fn(&mut T, EntityId, &mut ActionEncoder)`.
pub trait DropHook<T: ?Sized>: Send + Sync + 'static {
    /// Called when entity with component is dropped.
    fn on_drop(&self, component: &mut T, entity: EntityId, encoder: &mut ActionEncoder);
}

impl<T, F> DropHook<T> for F
where
    T: ?Sized,
    F: Fn(&mut T, EntityId, &mut ActionEncoder) + Send + Sync + 'static,
{
    fn on_drop(&self, component: &mut T, entity: EntityId, encoder: &mut ActionEncoder) {
        self(component, entity, encoder);
    }
}

/// Trait to be implemented by custom set hooks.
/// Has blanket implementation for `Fn(&mut T, &T, EntityId, &mut ActionEncoder)`.
pub trait SetHook<T: ?Sized>: Send + Sync + 'static {
    /// Called when new value is assigned to component instance.
    /// By default fallbacks to drop hook.
    fn on_set(
        &self,
        component: &mut T,
        value: &T,
        entity: EntityId,
        encoder: &mut ActionEncoder,
    ) -> bool;
}

impl<T, F> SetHook<T> for F
where
    T: ?Sized,
    F: Fn(&mut T, &T, EntityId, &mut ActionEncoder) -> bool + Send + Sync + 'static,
{
    fn on_set(
        &self,
        component: &mut T,
        value: &T,
        entity: EntityId,
        encoder: &mut ActionEncoder,
    ) -> bool {
        self(component, value, entity, encoder)
    }
}

/// Default drop hook type.
#[derive(Clone, Copy, Debug)]
pub struct DefaultDropHook;

impl<T> DropHook<T> for DefaultDropHook
where
    T: Component,
{
    fn on_drop(&self, component: &mut T, entity: EntityId, encoder: &mut ActionEncoder) {
        T::on_drop(component, entity, encoder);
    }
}

/// Default set hook type.
#[derive(Clone, Copy, Debug)]
pub struct DefaultSetHook;

impl<T> SetHook<T> for DefaultSetHook
where
    T: Component,
{
    fn on_set(&self, dst: &mut T, src: &T, entity: EntityId, encoder: &mut ActionEncoder) -> bool {
        T::on_set(dst, src, entity, encoder)
    }
}

/// Reference to [`ComponentInfo`] registered to [`ComponentRegistry`].
/// Allows user to setup custom drop and set hooks.
#[allow(missing_debug_implementations)]
pub struct ComponentInfoRef<
    'a,
    T: Component,
    D: DropHook<T> = DefaultDropHook,
    S: SetHook<T> = DefaultSetHook,
> {
    info: &'a mut ComponentInfo,
    phantom: PhantomData<T>,
    drop: ManuallyDrop<D>,
    set: ManuallyDrop<S>,
}

impl<'a, T, D, S> ComponentInfoRef<'a, T, D, S>
where
    T: Component,
    D: DropHook<T>,
    S: SetHook<T>,
{
    /// Configures drop hook for this component.
    /// Drop hook is executed when component is dropped.
    ///
    /// This hook is not executed on shutdown when `Archetype` is dropped.
    pub fn on_drop<F>(self, hook: F) -> ComponentInfoRef<'a, T, F, S>
    where
        F: DropHook<T>,
    {
        let me = ManuallyDrop::new(self);

        ComponentInfoRef {
            info: unsafe { ptr::read(&me.info) },
            phantom: me.phantom,
            drop: ManuallyDrop::new(hook),
            set: unsafe { ptr::read(&me.set) },
        }
    }
    /// Configures drop hook for this component.
    /// Drop hook is executed when component is dropped.
    ///
    /// This hook is not executed on shutdown when `Archetype` is dropped.
    pub fn on_drop_fn<F>(self, hook: F) -> ComponentInfoRef<'a, T, F, S>
    where
        F: Fn(&mut T, EntityId, &mut ActionEncoder) + Send + Sync + 'static,
    {
        self.on_drop(hook)
    }

    /// Configures set hook for this component.
    /// Set hook is executed when component is assigned a new value.
    ///
    /// By default, set hook is calling `on_drop`.
    pub fn on_set<F>(self, hook: F) -> ComponentInfoRef<'a, T, D, F>
    where
        F: SetHook<T>,
    {
        let me = ManuallyDrop::new(self);

        ComponentInfoRef {
            info: unsafe { ptr::read(&me.info) },
            phantom: me.phantom,
            drop: unsafe { ptr::read(&me.drop) },
            set: ManuallyDrop::new(hook),
        }
    }

    /// Configures set hook for this component.
    /// Set hook is executed when component is assigned a new value.
    ///
    /// By default, set hook is calling `on_drop`.
    pub fn on_set_fn<F>(self, hook: F) -> ComponentInfoRef<'a, T, D, F>
    where
        F: Fn(&mut T, &T, EntityId, &mut ActionEncoder) -> bool + Send + Sync + 'static,
    {
        self.on_set(hook)
    }
}

impl<T, D, S> Drop for ComponentInfoRef<'_, T, D, S>
where
    T: Component,
    D: DropHook<T>,
    S: SetHook<T>,
{
    #[inline]
    fn drop(&mut self) {
        self.info.drop_one = drop_one::<T, D>;
        self.info.on_drop =
            UnsafeAny::from_arc(Arc::new(unsafe { ManuallyDrop::take(&mut self.drop) }));
        self.info.set_one = set_one::<T, S, D>;
        self.info.on_set =
            UnsafeAny::from_arc(Arc::new(unsafe { ManuallyDrop::take(&mut self.set) }));
    }
}

/// Container for [`ComponentInfo`]s.
pub(crate) struct ComponentRegistry {
    components: HashMap<TypeId, ComponentInfo, NoOpHasherBuilder>,
}

impl ComponentRegistry {
    pub const fn new() -> Self {
        Self {
            components: HashMap::with_hasher(NoOpHasherBuilder),
        }
    }

    pub fn register<T>(&mut self) -> ComponentInfoRef<'_, T>
    where
        T: Component,
    {
        let info = self
            .components
            .entry(TypeId::of::<T>())
            .or_insert_with(ComponentInfo::of::<T>);

        ComponentInfoRef {
            info,
            phantom: PhantomData,
            drop: ManuallyDrop::new(DefaultDropHook),
            set: ManuallyDrop::new(DefaultSetHook),
        }
    }

    pub fn register_erased(&mut self, info: ComponentInfo) {
        self.components.entry(info.id).or_insert(info);
    }

    pub fn get_info(&self, id: TypeId) -> Option<&ComponentInfo> {
        self.components.get(&id)
    }
}

type DropOneFn = unsafe fn(&UnsafeAny, NonNull<u8>, EntityId, &mut ActionEncoder);
type SetOneFn =
    unsafe fn(&UnsafeAny, &UnsafeAny, NonNull<u8>, NonNull<u8>, EntityId, &mut ActionEncoder);
type FinalDrop = unsafe fn(NonNull<u8>, usize);

unsafe fn drop_one<T, D>(
    hook: &UnsafeAny,
    ptr: NonNull<u8>,
    entity: EntityId,
    encoder: &mut ActionEncoder,
) where
    T: Component,
    D: DropHook<T>,
{
    let mut ptr = ptr.cast::<T>();
    let hook = hook.downcast_ref_unchecked::<D>();
    hook.on_drop(ptr.as_mut(), entity, encoder);
    drop_in_place(ptr.as_mut());
}

unsafe fn set_one<T, S, D>(
    on_set: &UnsafeAny,
    on_drop: &UnsafeAny,
    dst: NonNull<u8>,
    src: NonNull<u8>,
    entity: EntityId,
    encoder: &mut ActionEncoder,
) where
    T: Component,
    S: SetHook<T>,
    D: DropHook<T>,
{
    let src = src.cast::<T>();
    let mut dst = dst.cast::<T>();
    let on_set = on_set.downcast_ref_unchecked::<S>();
    if on_set.on_set(dst.as_mut(), src.as_ref(), entity, encoder) {
        let on_drop = on_drop.downcast_ref_unchecked::<D>();
        on_drop.on_drop(dst.as_mut(), entity, encoder);
    }
    *dst.as_mut() = ptr::read(src.as_ref());
}

/// This drop is always called for all components when `Archetype` is dropped.
unsafe fn final_drop<T>(ptr: NonNull<u8>, count: usize) {
    drop_in_place(slice_from_raw_parts_mut(ptr.as_ptr(), count));
}
