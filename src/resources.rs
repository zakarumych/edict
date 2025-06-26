//!
//! Resources are singleton values that can be fetched from the [`World`](crate::world::World).
//!

use alloc::boxed::Box;
use core::{
    any::{type_name, Any, TypeId},
    borrow::{Borrow, BorrowMut},
    cmp::Ordering,
    fmt::{self, Debug, Display},
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
};

use atomicell::{AtomicCell, Ref, RefMut};
use hashbrown::HashMap;

use crate::type_id;

/// Resource borrowed immutably.
/// Derefs to the resource type.
///
/// # Example
///
/// ```
/// # use edict::{world::World, resources::Res};
///
/// let mut world = World::new();
///
/// world.insert_resource(42i32);
/// let value: Res<i32> = world.expect_resource();
/// assert_eq!(42, *value);
/// ```
#[repr(transparent)]
#[derive(Clone)]
pub struct Res<'a, T: ?Sized> {
    inner: Ref<'a, T>,
}

impl<'a, T> Deref for Res<'a, T>
where
    T: ?Sized,
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<T> fmt::Debug for Res<'_, T>
where
    T: Debug + ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <T as Debug>::fmt(self, f)
    }
}

impl<T> fmt::Display for Res<'_, T>
where
    T: Display + ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <T as Display>::fmt(self, f)
    }
}

impl<'a, T, U> PartialEq<U> for Res<'a, T>
where
    T: PartialEq<U> + ?Sized,
{
    #[inline]
    fn eq(&self, other: &U) -> bool {
        <T as PartialEq<U>>::eq(self, other)
    }
}

impl<'a, T, U> PartialOrd<U> for Res<'a, T>
where
    T: PartialOrd<U> + ?Sized,
{
    #[inline]
    fn partial_cmp(&self, other: &U) -> Option<Ordering> {
        <T as PartialOrd<U>>::partial_cmp(self, other)
    }
}

impl<'a, T> Hash for Res<'a, T>
where
    T: Hash + ?Sized,
{
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        <T as Hash>::hash(self, state)
    }
}

impl<'a, T> Borrow<T> for Res<'a, T>
where
    T: ?Sized,
{
    #[inline]
    fn borrow(&self) -> &T {
        self
    }
}

impl<'a, T, U> AsRef<U> for Res<'a, T>
where
    T: AsRef<U> + ?Sized,
{
    #[inline]
    fn as_ref(&self) -> &U {
        <T as AsRef<U>>::as_ref(self)
    }
}

impl<'a, T> Res<'a, T>
where
    T: ?Sized,
{
    /// Convert into a reference to the underlying data.
    /// This function will consume the `Res` and keep data borrowed.
    /// See [`World::undo_resource_leaks`](crate::world::World::undo_resource_leaks) to undo all leaks.
    ///
    /// This is an associated function that needs to be used as Res::leak(...).
    /// A method would interfere with methods of the same name on the resource used through [`Deref`].
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, resources::Res};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    ///
    /// // Leaking reference to resource causes it to stay borrowed.
    /// let value: &i32 = Res::leak(world.get_resource().unwrap());
    ///
    /// assert_eq!(42, *value);
    ///
    /// // Immutable borrow succeeds.
    /// assert_eq!(world.get_resource::<i32>().unwrap(), 42);
    /// ```
    ///
    /// ```should_panic
    /// # use edict::{world::World, resources::Res};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    ///
    /// // Leaking reference to resource causes it to stay borrowed.
    /// let value: &i32 = Res::leak(world.get_resource().unwrap());
    ///
    /// assert_eq!(42, *value);
    ///
    /// // Immutable borrow panics.
    /// world.get_resource_mut::<i32>();
    /// ```
    #[inline]
    pub fn leak(r: Res<'a, T>) -> &'a T {
        Ref::leak(r.inner)
    }
}

/// Resource borrowed mutably.
/// Derefs to the resource type.
///
/// # Example
///
/// ```
/// # use edict::{world::World, resources::ResMut};
///
/// let mut world = World::new();
///
/// world.insert_resource(42i32);
/// let mut value: ResMut<i32> = world.expect_resource_mut();
/// *value = 11;
/// ```
pub struct ResMut<'a, T: ?Sized> {
    inner: RefMut<'a, T>,
}

impl<'a, T> Deref for ResMut<'a, T>
where
    T: ?Sized,
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<'a, T> DerefMut for ResMut<'a, T>
where
    T: ?Sized,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.inner.deref_mut()
    }
}

impl<T> fmt::Debug for ResMut<'_, T>
where
    T: Debug + ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <T as Debug>::fmt(self, f)
    }
}

impl<T> fmt::Display for ResMut<'_, T>
where
    T: Display + ?Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <T as Display>::fmt(self, f)
    }
}

impl<'a, T, U> PartialEq<U> for ResMut<'a, T>
where
    T: PartialEq<U> + ?Sized,
{
    #[inline]
    fn eq(&self, other: &U) -> bool {
        <T as PartialEq<U>>::eq(self, other)
    }
}

impl<'a, T, U> PartialOrd<U> for ResMut<'a, T>
where
    T: PartialOrd<U> + ?Sized,
{
    #[inline]
    fn partial_cmp(&self, other: &U) -> Option<Ordering> {
        <T as PartialOrd<U>>::partial_cmp(self, other)
    }
}

impl<'a, T> Hash for ResMut<'a, T>
where
    T: Hash + ?Sized,
{
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        <T as Hash>::hash(self, state)
    }
}

impl<'a, T> Borrow<T> for ResMut<'a, T>
where
    T: ?Sized,
{
    #[inline]
    fn borrow(&self) -> &T {
        self
    }
}

impl<'a, T> BorrowMut<T> for ResMut<'a, T>
where
    T: ?Sized,
{
    #[inline]
    fn borrow_mut(&mut self) -> &mut T {
        self
    }
}

impl<'a, T, U> AsRef<U> for ResMut<'a, T>
where
    T: AsRef<U> + ?Sized,
{
    #[inline]
    fn as_ref(&self) -> &U {
        <T as AsRef<U>>::as_ref(self)
    }
}

impl<'a, T, U> AsMut<U> for ResMut<'a, T>
where
    T: AsMut<U> + ?Sized,
{
    #[inline]
    fn as_mut(&mut self) -> &mut U {
        <T as AsMut<U>>::as_mut(self)
    }
}

impl<'a, T> ResMut<'a, T>
where
    T: ?Sized,
{
    /// Convert into a mutable reference to the underlying data.
    /// This function will consume the `ResMut` and keep data borrowed.
    /// See [`World::undo_resource_leaks`](crate::world::World::undo_resource_leaks) to undo all leaks.
    ///
    /// This is an associated function that needs to be used as ResMut::leak(...).
    /// A method would interfere with methods of the same name on the resource used through [`Deref`].
    ///
    /// # Example
    ///
    /// ```
    /// # use edict::{world::World, resources::ResMut};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    ///
    /// // Leaking reference to resource causes it to stay borrowed.
    /// let value: &mut i32 = ResMut::leak(world.get_resource_mut().unwrap());
    ///
    /// *value == 11;
    /// ```
    ///
    /// ```should_panic
    /// # use edict::{world::World, resources::ResMut};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    ///
    /// // Leaking reference to resource causes it to stay borrowed.
    /// let value: &mut i32 = ResMut::leak(world.get_resource_mut().unwrap());
    ///
    /// // Immutable borrow panics.
    /// world.get_resource::<i32>();
    /// ```
    #[inline]
    pub fn leak(r: ResMut<'a, T>) -> &'a mut T {
        RefMut::leak(r.inner)
    }
}

struct Resource {
    // Box<AtomicCell> instead of AtomicCell<Box> to avoid false sharing
    data: Box<AtomicCell<dyn Any>>,
    name: &'static str,
}

impl Debug for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

// # Safety
//
// Only mutable references to `Send` type
// and immutable references to `Sync` type
// can be extracted using immutable reference to `Resource`.
//
// The generic marker dictates what traits to implement.
unsafe impl Sync for Resource {}

/// Type-erased container for singleton resources.
pub(crate) struct Resources {
    resources: HashMap<TypeId, Resource>,
}

impl Debug for Resources {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.resources)
    }
}

impl Resources {
    /// Returns a new empty resource container.
    #[inline]
    pub fn new() -> Self {
        Resources {
            resources: HashMap::new(),
        }
    }

    /// Inserts resource into container.
    /// Old value is replaced.
    ///
    /// For `Resources<Send>` resource type have to implement `Send`.
    /// Allowing `Resources<Send>` to be moved into another thread and drop resources there.
    ///
    /// `Resources<NoSend>` accepts any `'static` resource type.
    pub fn insert<T: 'static>(&mut self, resource: T) {
        let id = type_id::<T>();
        self.resources.insert(
            id,
            Resource {
                data: Box::new(AtomicCell::new(resource)),
                name: type_name::<T>(),
            },
        );
    }

    /// Inserts resource into container.
    /// Old value is replaced.
    ///
    /// For `Resources<Send>` resource type have to implement `Send`.
    /// Allowing `Resources<Send>` to be moved into another thread and drop resources there.
    ///
    /// `Resources<NoSend>` accepts any `'static` resource type.
    pub fn with<T: 'static>(&mut self, f: impl FnOnce() -> T) -> &mut T {
        let id = type_id::<T>();
        self.resources
            .entry(id)
            .or_insert_with(|| Resource {
                data: Box::new(AtomicCell::new(f())),
                name: type_name::<T>(),
            })
            .data
            .get_mut()
            .downcast_mut()
            .unwrap()
    }

    /// Removes resource from container.
    /// Returns `None` if resource is not found.
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        let mut resource = self.resources.remove(&type_id::<T>())?;
        let data = AtomicCell::into_inner(*unsafe {
            assert!(resource.data.get_mut().is::<T>());

            // # Safety
            // Manually casting is is safe, type behind `dyn Any` is `T`.
            Box::from_raw(Box::into_raw(resource.data) as *mut AtomicCell<T>)
        });
        Some(data)
    }

    /// Returns some reference to `Sync` resource.
    /// Returns none if resource is not found.
    #[inline]
    #[track_caller]
    pub fn get<T: Sync + 'static>(&self) -> Option<Res<'_, T>> {
        unsafe {
            // # Safety
            //
            // If `T` is `Sync` then this method is always safe.
            self.get_local()
        }
    }

    /// Returns some mutable reference to `Send` resource.
    /// Returns none if resource is not found.
    #[inline]
    #[track_caller]
    pub fn get_mut<T: Send + 'static>(&self) -> Option<ResMut<'_, T>> {
        unsafe {
            // # Safety
            //
            // If `T` is `Send` then this method is always safe.
            self.get_local_mut()
        }
    }

    /// Returns some reference to potentially `!Sync` resource.
    /// Returns none if resource is not found.
    ///
    /// # Safety
    ///
    /// User must ensure that obtained immutable reference is safe.
    /// For example calling this method from "main" thread is always safe.
    ///
    /// If `T` is `Sync` then this method is always safe.
    /// In this case prefer to use [`get`] method instead.
    #[inline]
    #[track_caller]
    pub unsafe fn get_local<T: 'static>(&self) -> Option<Res<'_, T>> {
        let id = type_id::<T>();

        let resource = self.resources.get(&id)?;

        let r = {
            // # Safety
            // Index from `ids` always valid.
            &resource.data
        }
        .try_borrow();

        let Some(r) = r else {
            panic!(
                "Attempt to borrow {} when it is already borrowed mutably",
                type_name::<T>()
            );
        };

        let r = Ref::map(r, |r| r.downcast_ref::<T>().unwrap());
        Some(Res { inner: r })
    }

    /// Returns some mutable reference to potentially `!Send` resource.
    /// Returns none if resource is not found.
    ///
    /// # Safety
    ///
    /// User must ensure that obtained mutable reference is safe.
    /// For example calling this method from "main" thread is always safe.
    ///
    /// If `T` is `Send` then this method is always safe.
    /// In this case prefer to use [`get_mut`] method instead.
    #[inline]
    #[track_caller]
    pub unsafe fn get_local_mut<T: 'static>(&self) -> Option<ResMut<'_, T>> {
        let id = type_id::<T>();

        let resource = self.resources.get(&id)?;

        let r = {
            // # Safety
            // Index from `ids` always valid.
            &resource.data
        }
        .try_borrow_mut();

        let Some(r) = r else {
            panic!(
                "Attempt to borrow {} mutably when it is already borrowed",
                type_name::<T>()
            );
        };

        let r = RefMut::map(r, |r| r.downcast_mut::<T>().unwrap());
        Some(ResMut { inner: r })
    }

    /// Reset all possible leaks on resources.
    /// Mutable reference guarantees that no borrows are active.
    pub fn undo_leaks(&mut self) {
        for (_, r) in self.resources.iter_mut() {
            r.data.undo_leak();
        }
    }

    /// Returns iterator over resource types.
    #[inline]
    pub fn resource_types(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.resources.keys().copied()
    }
}
