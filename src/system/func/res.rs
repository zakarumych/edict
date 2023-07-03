use core::{
    any::TypeId,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use atomicell::{Ref, RefMut};

use crate::{archetype::Archetype, query::Access, system::ActionQueue, world::World};

use super::{FnArg, FnArgState};

/// Function-system argument to fetch resource immutably.
#[repr(transparent)]
pub struct Res<'a, T: ?Sized> {
    inner: Ref<'a, T>,
}

impl<'a, T> Deref for Res<'a, T>
where
    T: ?Sized,
{
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<'a, T> Res<'a, T>
where
    T: ?Sized,
{
    /// Returns inner `Ref` guard.
    #[inline(always)]
    pub fn inner(self) -> Ref<'a, T> {
        self.inner
    }
}

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

    /// Returns access type to the specified component type this argument may perform.
    #[inline(always)]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    #[inline(always)]
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        if id == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> Res<'a, T> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        Res {
            inner: world.get_resource().expect("Missing resource"),
        }
    }
}

/// Function-system argument to fetch resource mutably.
pub struct ResMut<'a, T: ?Sized> {
    inner: RefMut<'a, T>,
}

impl<'a, T> Deref for ResMut<'a, T>
where
    T: ?Sized,
{
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<'a, T> DerefMut for ResMut<'a, T>
where
    T: ?Sized,
{
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        self.inner.deref_mut()
    }
}

impl<'a, T> ResMut<'a, T>
where
    T: ?Sized,
{
    /// Returns inner `Ref` guard.
    #[inline(always)]
    pub fn inner(self) -> RefMut<'a, T> {
        self.inner
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

    /// Returns access type to the specified component type this argument may perform.
    #[inline(always)]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    #[inline(always)]
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        if id == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> ResMut<'a, T> {
        // Safety: Declares read access.
        let world = unsafe { world.as_ref() };
        ResMut {
            inner: world.get_resource_mut().expect("Missing resource"),
        }
    }
}

/// Function-system argument to fetch resource immutably from "main" thread.
/// Can fetch `!Sync` resources.
/// Prefer using `Res` for `Sync` resources for better parallelism.
pub struct ResNoSync<'a, T: ?Sized> {
    inner: Ref<'a, T>,
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
    pub fn inner(self) -> Ref<'a, T> {
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

    /// Returns access type to the specified component type this argument may perform.
    #[inline(always)]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    #[inline(always)]
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        if id == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> ResNoSync<'a, T> {
        // Safety: Declares read.
        let world = unsafe { world.as_ref() };

        // Safety: Declares read access and local execution.
        let res = unsafe { world.get_local_resource() };
        ResNoSync {
            inner: res.expect("Missing resource"),
        }
    }
}

/// Function-system argument to fetch resource mutably from "main" thread.
/// Can fetch `!Send` resources.
/// Prefer using `ResMut` for `Send` resources for better parallelism.
pub struct ResMutNoSend<'a, T: ?Sized> {
    inner: RefMut<'a, T>,
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
    pub fn inner(self) -> RefMut<'a, T> {
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

    /// Returns access type to the specified component type this argument may perform.
    #[inline(always)]
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    #[inline(always)]
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        if id == TypeId::of::<T>() {
            Some(Access::Write)
        } else {
            None
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked<'a>(
        &'a mut self,
        world: NonNull<World>,
        _queue: &mut dyn ActionQueue,
    ) -> ResMutNoSend<'a, T> {
        // Safety: Declares read.
        let world = unsafe { world.as_ref() };

        // Safety: Declares read access and local execution.
        let res = unsafe { world.get_local_resource_mut() };
        ResMutNoSend {
            inner: res.expect("Missing resource"),
        }
    }
}
