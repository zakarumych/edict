use core::{
    any::{type_name, TypeId},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{
    archetype::Archetype,
    component::ComponentInfo,
    resources::{Res, ResMut},
    system::{Access, ActionBufferQueue},
    type_id,
    world::World,
};

use super::{FnArg, FnArgState};

/// State for [`Res`] argument.
pub struct ResState<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ResState<T> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<'a, T> FnArg for Res<'a, T>
where
    T: Sync + 'static,
{
    type State = ResState<T>;
}

unsafe impl<T> FnArgState for ResState<T>
where
    T: Sync + 'static,
{
    type Arg<'a> = Res<'a, T>;

    #[inline(always)]
    fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        false
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline(always)]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline(always)]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    /// Returns access type to the specified component type this argument may perform.
    #[inline(always)]
    fn component_access(&self, _comp: &ComponentInfo) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    #[inline(always)]
    fn resource_type_access(&self, ty: TypeId) -> Option<Access> {
        if ty == type_id::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> Res<'a, T> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        match world.get_resource() {
            Some(r) => r,
            None => missing_resource::<T>(),
        }
    }
}

/// State for [`ResMut`] argument
pub struct ResMutState<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ResMutState<T> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<'a, T> FnArg for ResMut<'a, T>
where
    T: Send + 'static,
{
    type State = ResMutState<T>;
}

unsafe impl<T> FnArgState for ResMutState<T>
where
    T: Send + 'static,
{
    type Arg<'a> = ResMut<'a, T>;

    #[inline(always)]
    fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        false
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline(always)]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline(always)]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    /// Returns access type to the specified component type this argument may perform.
    #[inline(always)]
    fn component_access(&self, _comp: &ComponentInfo) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    #[inline(always)]
    fn resource_type_access(&self, ty: TypeId) -> Option<Access> {
        if ty == type_id::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> ResMut<'a, T> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        match world.get_resource_mut() {
            Some(r) => r,
            None => missing_resource::<T>(),
        }
    }
}

/// Function-system argument to fetch resource immutably from "main" thread.
/// Can fetch `!Sync` resources.
/// Prefer using `Res` for `Sync` resources for better parallelism.
#[repr(transparent)]
pub struct ResNoSync<'a, T: ?Sized> {
    inner: Res<'a, T>,
}

impl<'a, T> Deref for ResNoSync<'a, T>
where
    T: ?Sized,
{
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<'a, T> ResNoSync<'a, T>
where
    T: ?Sized,
{
    /// Returns inner `Ref` guard.
    #[inline(always)]
    pub fn inner(self) -> Res<'a, T> {
        self.inner
    }
}

/// State for [`ResNoSync`] argument
pub struct ResNoSyncState<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ResNoSyncState<T> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<'a, T> FnArg for ResNoSync<'a, T>
where
    T: 'static,
{
    type State = ResNoSyncState<T>;
}

unsafe impl<T> FnArgState for ResNoSyncState<T>
where
    T: 'static,
{
    type Arg<'a> = ResNoSync<'a, T>;

    #[inline(always)]
    fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        true
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline(always)]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline(always)]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    /// Returns access type to the specified component type this argument may perform.
    #[inline(always)]
    fn component_access(&self, _comp: &ComponentInfo) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    #[inline(always)]
    fn resource_type_access(&self, ty: TypeId) -> Option<Access> {
        if ty == type_id::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> ResNoSync<'a, T> {
        // Safety: Declares read.
        let world = unsafe { world.as_ref() };

        // Safety: Declares read access and local execution.
        match unsafe { world.get_local_resource() } {
            Some(r) => ResNoSync { inner: r },
            None => missing_resource::<T>(),
        }
    }
}

/// Function-system argument to fetch resource mutably from "main" thread.
/// Can fetch `!Send` resources.
/// Prefer using `ResMut` for `Send` resources for better parallelism.
#[repr(transparent)]
pub struct ResMutNoSend<'a, T: ?Sized> {
    inner: ResMut<'a, T>,
}

impl<'a, T> Deref for ResMutNoSend<'a, T>
where
    T: ?Sized,
{
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<'a, T> DerefMut for ResMutNoSend<'a, T>
where
    T: ?Sized,
{
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        self.inner.deref_mut()
    }
}

impl<'a, T> ResMutNoSend<'a, T>
where
    T: ?Sized,
{
    /// Returns inner `Ref` guard.
    #[inline(always)]
    pub fn inner(self) -> ResMut<'a, T> {
        self.inner
    }
}

/// State for [`ResMutNoSend`] argument
pub struct ResMutNoSendState<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ResMutNoSendState<T> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<'a, T> FnArg for ResMutNoSend<'a, T>
where
    T: 'static,
{
    type State = ResMutNoSendState<T>;
}

unsafe impl<T> FnArgState for ResMutNoSendState<T>
where
    T: 'static,
{
    type Arg<'a> = ResMutNoSend<'a, T>;

    #[inline(always)]
    fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    fn is_local(&self) -> bool {
        true
    }

    #[inline(always)]
    fn world_access(&self) -> Option<Access> {
        Some(Access::Read)
    }

    #[inline(always)]
    fn visit_archetype(&self, _archetype: &Archetype) -> bool {
        false
    }

    #[inline(always)]
    fn borrows_components_at_runtime(&self) -> bool {
        false
    }

    /// Returns access type to the specified component type this argument may perform.
    #[inline(always)]
    fn component_access(&self, _comp: &ComponentInfo) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    #[inline(always)]
    fn resource_type_access(&self, ty: TypeId) -> Option<Access> {
        if ty == type_id::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionBufferQueue,
    ) -> ResMutNoSend<'a, T> {
        // Safety: Declares read.
        let world = unsafe { world.as_ref() };

        // Safety: Declares read access and local execution.
        match unsafe { world.get_local_resource_mut() } {
            Some(r) => ResMutNoSend { inner: r },
            None => missing_resource::<T>(),
        }
    }
}

fn missing_resource<T>() -> ! {
    panic!("Missing resource '{}'", type_name::<T>())
}
