use core::{
    any::TypeId,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use atomicell::{Ref, RefMut};

use crate::{archetype::Archetype, query::Access, world::World};

use super::{FnArg, FnArgCache, FnArgGet};

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
    pub fn inner(self) -> Ref<'a, T> {
        self.inner
    }
}

/// Cache for [`Res`] argument.
pub struct ResCache<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ResCache<T> {
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
    type Cache = ResCache<T>;
}

unsafe impl<'a, T> FnArgGet<'a> for ResCache<T>
where
    T: Sync + 'static,
{
    type Arg = Res<'a, T>;

    unsafe fn get_unchecked(&'a mut self, world: &'a World) -> Res<'a, T> {
        Res {
            inner: world.get_resource().expect("Missing resource"),
        }
    }
}

impl<T> FnArgCache for ResCache<T>
where
    T: Sync + 'static,
{
    fn is_local(&self) -> bool {
        false
    }

    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    /// Returns access type to the specified component type this argument may perform.
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        if id == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
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
    pub fn inner(self) -> RefMut<'a, T> {
        self.inner
    }
}

/// Cache for [`ResMut`] argument
pub struct ResMutCache<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ResMutCache<T> {
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
    type Cache = ResMutCache<T>;
}

unsafe impl<'a, T> FnArgGet<'a> for ResMutCache<T>
where
    T: Send + 'static,
{
    type Arg = ResMut<'a, T>;

    unsafe fn get_unchecked(&'a mut self, world: &'a World) -> ResMut<'a, T> {
        ResMut {
            inner: world.get_resource_mut().expect("Missing resource"),
        }
    }
}

impl<T> FnArgCache for ResMutCache<T>
where
    T: Send + 'static,
{
    fn is_local(&self) -> bool {
        false
    }

    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    /// Returns access type to the specified component type this argument may perform.
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        if id == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }
}

/// Function-system argument to fetch resource immutably from "main" thread.
/// Can fetch `!Sync` resources.
/// Prefer using `Res` for `Sync` resources for better parallelism.
pub struct ResNoSync<'a, T: ?Sized> {
    inner: RefMut<'a, T>,
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

impl<'a, T> DerefMut for ResNoSync<'a, T>
where
    T: ?Sized,
{
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        self.inner.deref_mut()
    }
}

impl<'a, T> ResNoSync<'a, T>
where
    T: ?Sized,
{
    /// Returns inner `Ref` guard.
    pub fn inner(self) -> RefMut<'a, T> {
        self.inner
    }
}

/// Cache for [`ResNoSync`] argument
pub struct ResNoSyncCache<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ResNoSyncCache<T> {
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
    type Cache = ResNoSyncCache<T>;
}

unsafe impl<'a, T> FnArgGet<'a> for ResNoSyncCache<T>
where
    T: 'static,
{
    type Arg = ResNoSync<'a, T>;

    unsafe fn get_unchecked(&'a mut self, world: &'a World) -> ResNoSync<'a, T> {
        ResNoSync {
            inner: world.get_local_resource_mut().expect("Missing resource"),
        }
    }
}

impl<T> FnArgCache for ResNoSyncCache<T>
where
    T: 'static,
{
    fn is_local(&self) -> bool {
        true
    }

    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    /// Returns access type to the specified component type this argument may perform.
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        if id == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
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
    pub fn inner(self) -> RefMut<'a, T> {
        self.inner
    }
}

/// Cache for [`ResMutNoSend`] argument
pub struct ResMutNoSendCache<T> {
    marker: PhantomData<fn() -> T>,
}

impl<T> Default for ResMutNoSendCache<T> {
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
    type Cache = ResMutNoSendCache<T>;
}

unsafe impl<'a, T> FnArgGet<'a> for ResMutNoSendCache<T>
where
    T: 'static,
{
    type Arg = ResMutNoSend<'a, T>;

    unsafe fn get_unchecked(&'a mut self, world: &'a World) -> ResMutNoSend<'a, T> {
        ResMutNoSend {
            inner: world.get_local_resource_mut().expect("Missing resource"),
        }
    }
}

impl<T> FnArgCache for ResMutNoSendCache<T>
where
    T: 'static,
{
    fn is_local(&self) -> bool {
        true
    }

    fn skips_archetype(&self, _archetype: &Archetype) -> bool {
        true
    }

    /// Returns access type to the specified component type this argument may perform.
    fn access_component(&self, _id: TypeId) -> Option<Access> {
        None
    }

    /// Returns access type to the specified resource type this argument may perform.
    fn access_resource(&self, id: TypeId) -> Option<Access> {
        if id == TypeId::of::<T>() {
            Some(Access::Read)
        } else {
            None
        }
    }
}
