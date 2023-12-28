//! This module defines [`Component`] trait and related types.

use alloc::{sync::Arc, vec, vec::Vec};
use core::{
    alloc::Layout,
    any::{type_name, Any, TypeId},
    borrow::{Borrow, BorrowMut},
    marker::PhantomData,
    mem::{transmute, ManuallyDrop},
    ptr::{self, drop_in_place, slice_from_raw_parts_mut, NonNull},
};

use hashbrown::hash_map::{Entry, HashMap};

use crate::{action::LocalActionEncoder, entity::EntityId, hash::NoOpHasherBuilder};

pub use edict_proc::Component;

#[doc(hidden)]
pub type BorrowFn<T> = for<'r> unsafe fn(NonNull<u8>, PhantomData<&'r ()>) -> &'r T;

#[doc(hidden)]
pub type BorrowFnMut<T> = for<'r> unsafe fn(NonNull<u8>, PhantomData<&'r mut ()>) -> &'r mut T;

/// Defines conversion of reference to component into reference to target type.
#[derive(Clone, Copy)]
pub struct ComponentBorrow {
    target: TypeId,

    // Actually is `BorrowFn<T>` where `TypeId::of::<T>() == target`.
    borrow: BorrowFn<()>,

    // Actually is `BorrowFnMut<T>` where `TypeId::of::<T>() == target`.
    borrow_mut: Option<BorrowFnMut<()>>,
}

// Types used by proc-macros
#[doc(hidden)]
pub mod private {
    use core::borrow::{Borrow, BorrowMut};

    use super::ComponentBorrow;

    pub struct DispatchBorrowMut<T, U>(pub DispatchBorrow<T, U>);

    pub struct DispatchBorrow<T, U>(pub core::marker::PhantomData<(T, U)>);

    impl<T, U> core::ops::Deref for DispatchBorrowMut<T, U> {
        type Target = DispatchBorrow<T, U>;

        fn deref(&self) -> &DispatchBorrow<T, U> {
            &self.0
        }
    }

    impl<T, U> DispatchBorrowMut<T, U>
    where
        T: BorrowMut<U> + Send + Sync + 'static,
        U: 'static,
    {
        pub fn insert(&self, extend: &mut impl core::iter::Extend<ComponentBorrow>) {
            extend.extend(Some(ComponentBorrow::make(
                |ptr: core::ptr::NonNull<u8>, core::marker::PhantomData| -> &U {
                    unsafe { ptr.cast::<T>().as_ref().borrow() }
                },
                core::option::Option::Some(
                    |ptr: core::ptr::NonNull<u8>, core::marker::PhantomData| -> &mut U {
                        unsafe { ptr.cast::<T>().as_mut().borrow_mut() }
                    },
                ),
            )));
        }
    }

    impl<T, U> DispatchBorrow<T, U>
    where
        T: Borrow<U> + Sync + 'static,
        U: 'static,
    {
        pub fn insert(&self, extend: &mut impl core::iter::Extend<ComponentBorrow>) {
            extend.extend(Some(ComponentBorrow::make(
                |ptr: core::ptr::NonNull<u8>, core::marker::PhantomData| -> &U {
                    unsafe { ptr.cast::<T>().as_ref().borrow() }
                },
                core::option::Option::None,
            )));
        }
    }

    impl<T, U> DispatchBorrowMut<T, U> {
        pub fn new() -> Self {
            DispatchBorrowMut(DispatchBorrow(core::marker::PhantomData::<(T, U)>))
        }
    }

    /// Extends output with `ComponentBorrow` to borrow borrowed type.
    /// Mutably if possible.
    #[macro_export]
    macro_rules! type_borrow {
        ($self:ty as $other:ty => $borrows:ident) => {
            let dispatch = $crate::component::private::DispatchBorrowMut::<$self, $other>::new();
            dispatch.insert(&mut $borrows);
        };
    }

    /// Extends output with `ComponentBorrow` to borrow dyn trait object.
    /// `dyn Trait + Send + Sync` and all valid combinations are automatically added.
    #[macro_export]
    macro_rules! trait_borrow {
        ($self:ty as $trait:path => $borrows:ident) => {{
            struct DispatchTraitBorrowSendSync<T> { send: DispatchTraitBorrowSend<T> }
            struct DispatchTraitBorrowSend<T>{ sync: DispatchTraitBorrowSync<T> }
            struct DispatchTraitBorrowSync<T>{ bare: DispatchTraitBorrow<T> }
            struct DispatchTraitBorrow<T>(core::marker::PhantomData<T>);

            impl<T> core::ops::Deref for DispatchTraitBorrowSendSync<T> {
                type Target = DispatchTraitBorrowSend<T>;

                fn deref(&self) -> &DispatchTraitBorrowSend<T> {
                    &self.send
                }
            }

            impl<T> core::ops::Deref for DispatchTraitBorrowSend<T> {
                type Target = DispatchTraitBorrowSync<T>;

                fn deref(&self) -> &DispatchTraitBorrowSync<T> {
                    &self.sync
                }
            }

            impl<T> core::ops::Deref for DispatchTraitBorrowSync<T> {
                type Target = DispatchTraitBorrow<T>;

                fn deref(&self) -> &DispatchTraitBorrow<T> {
                    &self.bare
                }
            }

            impl<T> DispatchTraitBorrowSendSync<T> {
                fn new() -> Self {
                    DispatchTraitBorrowSendSync {
                        send: DispatchTraitBorrowSend {
                            sync: DispatchTraitBorrowSync {
                                bare: DispatchTraitBorrow(core::marker::PhantomData::<T>),
                            },
                        },

                    }
                }
            }

            impl<T: $trait + $crate::private::Send + $crate::private::Sync + 'static> DispatchTraitBorrowSendSync<T> {
                #[allow(unused)]
                fn insert(
                    &self,
                    extend: &mut $crate::private::Vec<$crate::component::ComponentBorrow>,
                ) {
                    self.insert_one(extend);
                    self.send.insert_one(extend);
                    self.sync.insert_one(extend);
                    self.bare.insert_one(extend);
                }

                #[allow(unused)]
                fn insert_one(
                    &self,
                    extend: &mut $crate::private::Vec<$crate::component::ComponentBorrow>,
                ) {
                    extend.extend(Some($crate::component::ComponentBorrow::make(
                        |ptr: $crate::private::NonNull<u8>,
                         $crate::private::PhantomData|
                         -> &(dyn $trait + $crate::private::Send + $crate::private::Sync) {
                            unsafe { ptr.cast::<T>().as_ref() }
                        },
                        $crate::private::Option::Some(
                            |ptr: $crate::private::NonNull<u8>,
                             $crate::private::PhantomData|
                             -> &mut (dyn $trait + $crate::private::Send + $crate::private::Sync) {
                                unsafe { ptr.cast::<T>().as_mut() }
                            },
                        ),
                    )));
                }
            }

            impl<T: $trait + $crate::private::Send + 'static> DispatchTraitBorrowSend<T> {
                #[allow(unused)]
                fn insert(
                    &self,
                    extend: &mut $crate::private::Vec<$crate::component::ComponentBorrow>,
                ) {
                    self.insert_one(extend);
                    self.bare.insert_one(extend);
                }

                #[allow(unused)]
                fn insert_one(
                    &self,
                    extend: &mut $crate::private::Vec<$crate::component::ComponentBorrow>,
                ) {
                    extend.extend(Some($crate::component::ComponentBorrow::make(
                        |ptr: $crate::private::NonNull<u8>,
                         $crate::private::PhantomData|
                         -> &(dyn $trait + Send) {
                            unsafe { ptr.cast::<T>().as_ref() }
                        },
                        $crate::private::Option::Some(
                            |ptr: $crate::private::NonNull<u8>,
                             $crate::private::PhantomData|
                             -> &mut (dyn $trait + Send) {
                                unsafe { ptr.cast::<T>().as_mut() }
                            },
                        ),
                    )));
                }
            }

            impl<T: $trait + $crate::private::Sync + 'static> DispatchTraitBorrowSync<T> {
                #[allow(unused)]
                fn insert(
                    &self,
                    extend: &mut $crate::private::Vec<$crate::component::ComponentBorrow>,
                ) {
                    self.insert_one(extend);
                    self.bare.insert_one(extend);
                }

                #[allow(unused)]
                fn insert_one(
                    &self,
                    extend: &mut $crate::private::Vec<$crate::component::ComponentBorrow>,
                ) {
                    extend.extend(Some($crate::component::ComponentBorrow::make(
                        |ptr: $crate::private::NonNull<u8>,
                         $crate::private::PhantomData|
                         -> &(dyn $trait + Sync) {
                            unsafe { ptr.cast::<T>().as_ref() }
                        },
                        $crate::private::Option::Some(
                            |ptr: $crate::private::NonNull<u8>,
                             $crate::private::PhantomData|
                             -> &mut (dyn $trait + Sync) {
                                unsafe { ptr.cast::<T>().as_mut() }
                            },
                        ),
                    )));
                }
            }

            impl<T: $trait + 'static> DispatchTraitBorrow<T> {
                #[allow(unused)]
                fn insert(
                    &self,
                    extend: &mut $crate::private::Vec<$crate::component::ComponentBorrow>,
                ) {
                    self.insert_one(extend);
                }

                fn insert_one(
                    &self,
                    extend: &mut $crate::private::Vec<$crate::component::ComponentBorrow>,
                ) {
                    extend.extend(Some($crate::component::ComponentBorrow::make(
                        |ptr: $crate::private::NonNull<u8>, $crate::private::PhantomData| -> &dyn $trait {
                            unsafe { ptr.cast::<T>().as_ref() }
                        },
                        $crate::private::Option::Some(
                            |ptr: $crate::private::NonNull<u8>,
                             $crate::private::PhantomData|
                             -> &mut dyn $trait {
                                unsafe { ptr.cast::<T>().as_mut() }
                            },
                        ),
                    )));
                }
            }

            let dispatch = DispatchTraitBorrowSendSync::<$self>::new();
            dispatch.insert(&mut $borrows);
        }};
    }
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
        T: Borrow<U> + 'static,
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
        T: 'static,
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

/// Defines component properties and behavior.
/// Types may implement this trait to act as components and support implicit self-registration.
///
/// Types that do not implement this trait may still be used as components.
/// They are called "external", must be registered manually and require usage of
/// [`World::spawn_external`], [`World::insert_external`], and [`World::insert_external_bundle`] methods that would panic
/// when component type is not yet registered. Even if the type happens to implement [`Component`] trait.
///
/// [`World::spawn_external`]: edict::world::World::spawn_external
/// [`World::insert_external`]: edict::world::World::insert_external
/// [`World::insert_external_bundle`]: edict::world::World::insert_external_bundle
pub trait Component: Sized + 'static {
    /// Returns name of the component type.
    #[inline(always)]
    fn name() -> &'static str {
        core::any::type_name::<Self>()
    }

    /// Hook that is executed when entity with component is dropped.
    #[inline(always)]
    fn on_drop(&mut self, id: EntityId, encoder: LocalActionEncoder) {
        let _ = id;
        let _ = encoder;
    }

    /// Hook that is executed whenever new value is assigned to the component.
    /// If this method returns `true` then `on_remove` is executed for old value before assignment.
    #[inline(always)]
    fn on_replace(&mut self, value: &Self, id: EntityId, encoder: LocalActionEncoder) -> bool
    where
        Self: Sized,
    {
        let _ = value;
        let _ = id;
        let _ = encoder;
        true
    }

    /// Returns array of component borrows supported by the type.
    #[inline(always)]
    fn borrows() -> Vec<ComponentBorrow> {
        vec![ComponentBorrow::auto::<Self>()]
    }
}

/// Type information required for components.
#[derive(Clone)]
pub struct ComponentInfo {
    /// [`TypeId`] of the component.
    ty: TypeId,

    /// [`Layout`] of the component.
    layout: Layout,

    /// Name of the component.
    name: &'static str,

    /// Function that calls drop glue for a component.
    /// Supports custom hooks.
    drop_one: DropOneFn,

    /// Context for `drop_one` command when component is dropped.
    on_drop: Arc<dyn Any + Send + Sync>,

    /// Function that replaces component at target location.
    /// Supports custom hooks.
    set_one: SetOneFn,

    /// Context for `set_one` command.
    on_replace: Arc<dyn Any + Send + Sync>,

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
            ty: TypeId::of::<T>(),
            layout: Layout::new::<T>(),
            name: T::name(),
            drop_one: drop_one::<T, DefaultDropHook>,
            on_drop: Arc::new(DefaultDropHook),
            set_one: set_one::<T, DefaultSetHook, DefaultDropHook>,
            on_replace: Arc::new(DefaultSetHook),
            final_drop: final_drop::<T>,
            borrows: Arc::from(T::borrows()),
        }
    }

    /// Returns component information for specified external type.
    #[inline(always)]
    pub fn external<T>() -> Self
    where
        T: 'static,
    {
        ComponentInfo {
            ty: TypeId::of::<T>(),
            layout: Layout::new::<T>(),
            name: type_name::<T>(),
            drop_one: drop_one::<T, ExternalDropHook>,
            on_drop: Arc::new(ExternalDropHook),
            set_one: set_one::<T, ExternalSetHook, ExternalDropHook>,
            on_replace: Arc::new(ExternalSetHook),
            final_drop: final_drop::<T>,
            borrows: Arc::new([]),
        }
    }

    #[inline(always)]
    pub(crate) fn id(&self) -> TypeId {
        self.ty
    }

    #[inline(always)]
    pub(crate) fn layout(&self) -> Layout {
        self.layout
    }

    #[inline(always)]
    pub(crate) fn drop_one(&self, ptr: NonNull<u8>, id: EntityId, encoder: LocalActionEncoder) {
        unsafe {
            (self.drop_one)(NonNull::from(&*self.on_drop).cast(), ptr, id, encoder);
        }
    }

    #[inline(always)]
    pub(crate) fn set_one(
        &self,
        dst: NonNull<u8>,
        src: NonNull<u8>,
        id: EntityId,
        encoder: LocalActionEncoder,
    ) {
        unsafe {
            (self.set_one)(
                NonNull::from(&*self.on_replace).cast(),
                NonNull::from(&*self.on_drop).cast(),
                dst,
                src,
                id,
                encoder,
            );
        }
    }

    #[inline(always)]
    pub(crate) fn final_drop(&self, ptr: NonNull<u8>, count: usize) {
        unsafe {
            (self.final_drop)(ptr, count);
        }
    }

    #[inline(always)]
    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    #[inline(always)]
    pub(crate) fn borrows(&self) -> &[ComponentBorrow] {
        &self.borrows
    }
}

/// Trait to be implemented by custom drop hooks.
/// Has blanket implementation for `Fn(&mut T, EntityId, LocalActionEncoder)`.
pub trait DropHook<T: ?Sized>: Send + Sync + 'static {
    /// Called when entity with component is dropped.
    fn on_drop(&self, component: &mut T, id: EntityId, encoder: LocalActionEncoder);
}

impl<T, F> DropHook<T> for F
where
    T: ?Sized,
    F: Fn(&mut T, EntityId, LocalActionEncoder) + Send + Sync + 'static,
{
    #[inline(always)]
    fn on_drop(&self, component: &mut T, id: EntityId, encoder: LocalActionEncoder) {
        self(component, id, encoder);
    }
}

/// Trait to be implemented by custom set hooks.
/// Has blanket implementation for `Fn(&mut T, &T, EntityId, LocalActionEncoder)`.
pub trait SetHook<T: ?Sized>: Send + Sync + 'static {
    /// Called when new value is assigned to component instance.
    /// By default fallbacks to drop hook.
    fn on_replace(
        &self,
        component: &mut T,
        value: &T,
        id: EntityId,
        encoder: LocalActionEncoder,
    ) -> bool;
}

impl<T, F> SetHook<T> for F
where
    T: ?Sized,
    F: Fn(&mut T, &T, EntityId, LocalActionEncoder) -> bool + Send + Sync + 'static,
{
    #[inline(always)]
    fn on_replace(
        &self,
        component: &mut T,
        value: &T,
        id: EntityId,
        encoder: LocalActionEncoder,
    ) -> bool {
        self(component, value, id, encoder)
    }
}

/// Default drop hook type.
#[derive(Clone, Copy, Debug)]
pub struct DefaultDropHook;

impl<T> DropHook<T> for DefaultDropHook
where
    T: Component,
{
    #[inline(always)]
    fn on_drop(&self, component: &mut T, id: EntityId, encoder: LocalActionEncoder) {
        T::on_drop(component, id, encoder);
    }
}

/// Default set hook type.
#[derive(Clone, Copy, Debug)]
pub struct DefaultSetHook;

impl<T> SetHook<T> for DefaultSetHook
where
    T: Component,
{
    #[inline(always)]
    fn on_replace(&self, dst: &mut T, src: &T, id: EntityId, encoder: LocalActionEncoder) -> bool {
        T::on_replace(dst, src, id, encoder)
    }
}

/// External drop hook type.
#[derive(Clone, Copy, Debug)]
pub struct ExternalDropHook;

impl<T> DropHook<T> for ExternalDropHook {
    #[inline(always)]
    fn on_drop(&self, _component: &mut T, _id: EntityId, _encoder: LocalActionEncoder) {}
}

/// External set hook type.
#[derive(Clone, Copy, Debug)]
pub struct ExternalSetHook;

impl<T> SetHook<T> for ExternalSetHook {
    #[inline(always)]
    fn on_replace(
        &self,
        _dst: &mut T,
        _src: &T,
        _entity: EntityId,
        _encoder: LocalActionEncoder,
    ) -> bool {
        true
    }
}

/// Reference to registered [`ComponentInfo`].
/// Allows user to setup custom drop and set hooks.
pub struct ComponentInfoRef<
    'a,
    T: 'static,
    D: DropHook<T> = DefaultDropHook,
    S: SetHook<T> = DefaultSetHook,
> {
    info: Option<&'a mut ComponentInfo>,
    phantom: PhantomData<T>,
    drop: ManuallyDrop<D>,
    set: ManuallyDrop<S>,
    name: Option<&'static str>,
}

impl<T, D, S> Drop for ComponentInfoRef<'_, T, D, S>
where
    T: 'static,
    D: DropHook<T>,
    S: SetHook<T>,
{
    #[inline(always)]
    fn drop(&mut self) {
        self.drop_impl();
    }
}

impl<'a, T, D, S> ComponentInfoRef<'a, T, D, S>
where
    T: 'static,
    D: DropHook<T>,
    S: SetHook<T>,
{
    #[inline(always)]
    fn drop_impl(&mut self) {
        let info = self.info.as_mut().unwrap();
        info.drop_one = drop_one::<T, D>;
        info.on_drop = Arc::new(unsafe { ManuallyDrop::take(&mut self.drop) });
        info.set_one = set_one::<T, S, D>;
        info.on_replace = Arc::new(unsafe { ManuallyDrop::take(&mut self.set) });
        if let Some(name) = self.name {
            info.name = name;
        }
    }

    /// Finishes component registration.
    /// Returns resulting [`ComponentInfo`]
    pub fn finish(self) -> &'a ComponentInfo {
        let mut me = ManuallyDrop::new(self);
        me.drop_impl();
        me.info.take().unwrap()
    }

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
            name: me.name,
        }
    }

    /// Configures drop hook for this component.
    /// Drop hook is executed when component is dropped.
    ///
    /// This hook is not executed on shutdown when `Archetype` is dropped.
    pub fn on_drop_fn<F>(self, hook: F) -> ComponentInfoRef<'a, T, F, S>
    where
        F: Fn(&mut T, EntityId, LocalActionEncoder) + Send + Sync + 'static,
    {
        self.on_drop(hook)
    }

    /// Configures set hook for this component.
    /// Set hook is executed when component is assigned a new value.
    ///
    /// By default, set hook is calling `on_drop`.
    pub fn on_replace<F>(self, hook: F) -> ComponentInfoRef<'a, T, D, F>
    where
        F: SetHook<T>,
    {
        let me = ManuallyDrop::new(self);

        ComponentInfoRef {
            info: unsafe { ptr::read(&me.info) },
            phantom: me.phantom,
            drop: unsafe { ptr::read(&me.drop) },
            set: ManuallyDrop::new(hook),
            name: me.name,
        }
    }

    /// Configures set hook for this component.
    /// Set hook is executed when component is assigned a new value.
    ///
    /// By default, set hook is calling `on_drop`.
    pub fn on_replace_fn<F>(self, hook: F) -> ComponentInfoRef<'a, T, D, F>
    where
        F: Fn(&mut T, &T, EntityId, LocalActionEncoder) -> bool + Send + Sync + 'static,
    {
        self.on_replace(hook)
    }

    /// Overrides default component type name.
    pub fn name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
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

    pub fn get_or_register<T>(&mut self) -> &ComponentInfo
    where
        T: Component,
    {
        self.components
            .entry(TypeId::of::<T>())
            .or_insert_with(ComponentInfo::of::<T>)
    }

    pub fn get_or_register_raw(&mut self, info: ComponentInfo) -> &ComponentInfo {
        self.components.entry(info.id()).or_insert(info)
    }

    pub fn register_raw(&mut self, info: ComponentInfo) {
        match self.components.entry(info.id()) {
            Entry::Occupied(_) => panic!("Component already registered"),
            Entry::Vacant(e) => {
                e.insert(info);
            }
        }
    }

    pub fn ensure_component_registered<T>(&mut self)
    where
        T: Component,
    {
        self.get_or_register_raw(ComponentInfo::of::<T>());
    }

    pub fn ensure_external_registered<T>(&mut self)
    where
        T: 'static,
    {
        self.get_or_register_raw(ComponentInfo::external::<T>());
    }

    pub fn register_component<'a, T>(&'a mut self) -> ComponentInfoRef<'a, T>
    where
        T: Component,
    {
        let info = match self.components.entry(TypeId::of::<T>()) {
            Entry::Occupied(_) => panic!("Component already registered"),
            Entry::Vacant(e) => e.insert(ComponentInfo::of::<T>()),
        };

        ComponentInfoRef {
            info: Some(info),
            phantom: PhantomData,
            drop: ManuallyDrop::new(DefaultDropHook),
            set: ManuallyDrop::new(DefaultSetHook),
            name: None,
        }
    }

    pub fn register_external<'a, T>(
        &'a mut self,
    ) -> ComponentInfoRef<'a, T, ExternalDropHook, ExternalSetHook>
    where
        T: 'static,
    {
        let info = match self.components.entry(TypeId::of::<T>()) {
            Entry::Occupied(_) => panic!("Component already registered"),
            Entry::Vacant(e) => e.insert(ComponentInfo::external::<T>()),
        };

        ComponentInfoRef {
            info: Some(info),
            phantom: PhantomData,
            drop: ManuallyDrop::new(ExternalDropHook),
            set: ManuallyDrop::new(ExternalSetHook),
            name: None,
        }
    }

    pub fn get_info(&self, ty: TypeId) -> Option<&ComponentInfo> {
        self.components.get(&ty)
    }

    pub fn iter_info(&self) -> impl Iterator<Item = &ComponentInfo> {
        self.components.values()
    }
}

struct Opaque;

type DropOneFn = unsafe fn(NonNull<Opaque>, NonNull<u8>, EntityId, LocalActionEncoder);
type SetOneFn = unsafe fn(
    NonNull<Opaque>,
    NonNull<Opaque>,
    NonNull<u8>,
    NonNull<u8>,
    EntityId,
    LocalActionEncoder,
);
type FinalDrop = unsafe fn(NonNull<u8>, usize);

unsafe fn drop_one<T, D>(
    hook: NonNull<Opaque>,
    ptr: NonNull<u8>,
    id: EntityId,
    encoder: LocalActionEncoder,
) where
    T: 'static,
    D: DropHook<T>,
{
    let mut ptr = ptr.cast::<T>();
    let hook = unsafe { hook.cast::<D>().as_ref() };
    let value = unsafe { ptr.as_mut() };
    hook.on_drop(value, id, encoder);
    unsafe {
        drop_in_place(value);
    }
}

unsafe fn set_one<T, S, D>(
    on_replace: NonNull<Opaque>,
    on_drop: NonNull<Opaque>,
    dst: NonNull<u8>,
    src: NonNull<u8>,
    id: EntityId,
    mut encoder: LocalActionEncoder,
) where
    T: 'static,
    S: SetHook<T>,
    D: DropHook<T>,
{
    let src = unsafe { src.cast::<T>().as_ref() };
    let dst = unsafe { dst.cast::<T>().as_mut() };
    let on_replace = unsafe { on_replace.cast::<S>().as_ref() };
    if on_replace.on_replace(dst, src, id, encoder.reborrow()) {
        let on_drop = unsafe { on_drop.cast::<D>().as_ref() };
        on_drop.on_drop(dst, id, encoder);
    }
    unsafe {
        *dst = ptr::read(src);
    }
}

/// This drop is always called for all components when `Archetype` is dropped.
/// Does not invoke any hooks.
unsafe fn final_drop<T>(ptr: NonNull<u8>, count: usize) {
    let slice = slice_from_raw_parts_mut(ptr.cast::<T>().as_ptr(), count);
    unsafe {
        drop_in_place(slice);
    }
}

/// Value component properties and behavior.
pub trait Value: 'static {
    /// Returns name of the component type.
    fn name(&self) -> &'static str;
}

impl<T> Value for T
where
    T: Component,
{
    #[inline(always)]
    fn name(&self) -> &'static str {
        T::name()
    }
}
