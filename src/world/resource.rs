use core::any::{type_name, TypeId};

use crate::resources::{Res, ResMut};

use super::{World, WorldLocal};

impl World {
    /// Inserts resource instance.
    /// Old value is replaced.
    ///
    /// To access resource, use [`World::get_resource`] and [`World::get_resource_mut`] methods.
    ///
    /// [`World::get_resource`]: struct.World.html#method.get_resource
    /// [`World::get_resource_mut`]: struct.World.html#method.get_resource_mut
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// *world.get_resource_mut::<i32>().unwrap() = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn insert_resource<T: 'static>(&mut self, resource: T) {
        self.resources.insert(resource)
    }

    /// Returns reference to the resource instance.
    /// Inserts new instance if it does not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let value = world.with_resource(|| 42i32);
    /// assert_eq!(*value, 42);
    /// *value = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn with_resource<T: 'static>(&mut self, f: impl FnOnce() -> T) -> &mut T {
        self.resources.with(f)
    }

    /// Returns reference to the resource instance.
    /// Inserts new instance if it does not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// let value = world.with_default_resource::<u32>();
    /// assert_eq!(*value, 0);
    /// *value = 11;
    /// assert_eq!(*world.get_resource::<u32>().unwrap(), 11);
    /// ```
    pub fn with_default_resource<T: Default + 'static>(&mut self) -> &mut T {
        self.resources.with(T::default)
    }

    /// Remove resource instance.
    /// Returns `None` if resource was not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// world.remove_resource::<i32>();
    /// assert!(world.get_resource::<i32>().is_none());
    /// ```
    pub fn remove_resource<T: 'static>(&mut self) -> Option<T> {
        self.resources.remove()
    }

    /// Returns some reference to potentially `!Sync` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// # use core::cell::Cell;
    /// let mut world = World::new();
    /// world.insert_resource(Cell::new(42i32));
    /// unsafe {
    ///     assert_eq!(42, world.get_local_resource::<Cell<i32>>().unwrap().get());
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// User must ensure that obtained immutable reference is safe.
    /// For example calling this method from "main" thread is always safe.
    ///
    /// If `T` is `Sync` then this method is also safe.
    /// In this case prefer to use [`World::get_resource`] method instead.
    ///
    /// If user has mutable access to [`World`] this function is guaranteed to be safe to call.
    /// [`WorldLocal`] wrapper can be used to avoid `unsafe` blocks.
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// let local = world.local();
    /// assert_eq!(42, *local.get_resource::<i32>().unwrap());
    /// ```
    #[track_caller]
    pub unsafe fn get_local_resource<T: 'static>(&self) -> Option<Res<T>> {
        unsafe { self.resources.get_local::<T>() }
    }

    /// Returns some mutable reference to potentially `!Send` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// # use core::cell::Cell;
    /// let mut world = World::new();
    /// world.insert_resource(Cell::new(42i32));
    /// unsafe {
    ///     *world.get_local_resource_mut::<Cell<i32>>().unwrap().get_mut() = 11;
    ///     assert_eq!(11, world.get_local_resource::<Cell<i32>>().unwrap().get());
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// User must ensure that obtained mutable reference is safe.
    /// For example calling this method from "main" thread is always safe.
    ///
    /// If `T` is `Send` then this method is also safe.
    /// In this case prefer to use [`World::get_resource_mut`] method instead.
    ///
    /// If user has mutable access to [`World`] this function is guaranteed to be safe to call.
    /// [`WorldLocal`] wrapper can be used to avoid `unsafe` blocks.
    ///
    /// ```
    /// # use edict::world::{World, WorldLocal};
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// let local = world.local();
    /// *local.get_resource_mut::<i32>().unwrap() = 11;
    /// ```
    #[track_caller]
    pub unsafe fn get_local_resource_mut<T: 'static>(&self) -> Option<ResMut<T>> {
        unsafe { self.resources.get_local_mut::<T>() }
    }

    /// Returns some reference to `Sync` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// assert!(world.get_resource::<i32>().is_none());
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// ```
    #[track_caller]
    pub fn get_resource<T: Sync + 'static>(&self) -> Option<Res<T>> {
        self.resources.get::<T>()
    }

    /// Returns reference to `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.expect_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.expect_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn expect_resource<T: Sync + 'static>(&self) -> Res<T> {
        match self.resources.get::<T>() {
            Some(res) => res,
            None => panic!("Resource {} not found", type_name::<T>()),
        }
    }

    /// Returns a copy for the `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.copy_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(world.copy_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn copy_resource<T: Copy + Sync + 'static>(&self) -> T {
        match self.resources.get::<T>() {
            Some(res) => *res,
            None => panic!("Resource {} not found", type_name::<T>()),
        }
    }

    /// Returns a clone for the `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.copy_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(world.copy_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn clone_resource<T: Clone + Sync + 'static>(&self) -> T {
        match self.resources.get::<T>() {
            Some(res) => (*res).clone(),
            None => panic!("Resource {} not found", type_name::<T>()),
        }
    }

    /// Returns some mutable reference to `Send` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// assert!(world.get_resource_mut::<i32>().is_none());
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// *world.get_resource_mut::<i32>().unwrap() = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    #[track_caller]
    pub fn get_resource_mut<T: Send + 'static>(&self) -> Option<ResMut<T>> {
        self.resources.get_mut::<T>()
    }

    /// Returns mutable reference to `Send` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.expect_resource_mut::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// *world.expect_resource_mut::<i32>() = 11;
    /// assert_eq!(*world.expect_resource_mut::<i32>(), 11);
    /// ```
    #[track_caller]
    pub fn expect_resource_mut<T: Send + 'static>(&self) -> ResMut<T> {
        match self.resources.get_mut::<T>() {
            Some(res) => res,
            None => panic!("Resource {} not found", type_name::<T>()),
        }
    }

    /// Reset all possible leaks on resources.
    /// Mutable reference guarantees that no borrows are active.
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
    /// *value = 11;
    ///
    /// // Reset all borrows including leaked ones.
    /// world.undo_resource_leaks();
    ///
    /// // Borrow succeeds.
    /// assert_eq!(world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn undo_resource_leaks(&mut self) {
        self.resources.undo_leaks()
    }

    /// Returns iterator over resource types.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::{any::TypeId, collections::HashSet};
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// world.insert_resource(1.5f32);
    /// assert_eq!(
    ///     world.resource_types().collect::<HashSet<_>>(),
    ///     HashSet::from([TypeId::of::<i32>(), TypeId::of::<f32>()]),
    /// );
    /// ```
    pub fn resource_types(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.resources.resource_types()
    }
}

impl WorldLocal {
    /// Inserts resource instance.
    /// Old value is replaced.
    ///
    /// To access resource, use [`World::get_resource`] and [`World::get_resource_mut`] methods.
    ///
    /// [`World::get_resource`]: struct.World.html#method.get_resource
    /// [`World::get_resource_mut`]: struct.World.html#method.get_resource_mut
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// *world.get_resource_mut::<i32>().unwrap() = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    pub fn insert_resource_defer<T: 'static>(&self, resource: T) {
        self.defer(|world| {
            world.insert_resource(resource);
        })
    }

    /// Drops resource instance.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// world.remove_resource::<i32>();
    /// assert!(world.get_resource::<i32>().is_none());
    /// ```
    pub fn drop_resource_defer<T: 'static>(&self) {
        self.defer(|world| {
            world.remove_resource::<T>();
        });
    }

    /// Returns some reference to `Sync` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// assert!(world.get_resource::<i32>().is_none());
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 42);
    /// ```
    #[track_caller]
    pub fn get_resource<T: 'static>(&self) -> Option<Res<T>> {
        unsafe { self.inner.resources.get_local::<T>() }
    }

    /// Returns reference to `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.expect_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(*world.expect_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn expect_resource<T: 'static>(&self) -> Res<T> {
        match unsafe { self.inner.resources.get_local::<T>() } {
            Some(res) => res,
            None => panic!("Resource {} not found", type_name::<T>()),
        }
    }

    /// Returns a copy for the `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.copy_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(world.copy_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn copy_resource<T: Copy + 'static>(&self) -> T {
        match unsafe { self.inner.resources.get_local::<T>() } {
            Some(res) => *res,
            None => panic!("Resource {} not found", type_name::<T>()),
        }
    }

    /// Returns a clone for the `Sync` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.copy_resource::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// assert_eq!(world.copy_resource::<i32>(), 42);
    /// ```
    #[track_caller]
    pub fn clone_resource<T: Clone + 'static>(&self) -> T {
        match unsafe { self.inner.resources.get_local::<T>() } {
            Some(res) => (*res).clone(),
            None => panic!("Resource {} not found", type_name::<T>()),
        }
    }

    /// Returns some mutable reference to `Send` resource.
    /// Returns none if resource is not found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// assert!(world.get_resource_mut::<i32>().is_none());
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// *world.get_resource_mut::<i32>().unwrap() = 11;
    /// assert_eq!(*world.get_resource::<i32>().unwrap(), 11);
    /// ```
    #[track_caller]
    pub fn get_resource_mut<T: 'static>(&self) -> Option<ResMut<T>> {
        unsafe { self.inner.resources.get_local_mut::<T>() }
    }

    /// Returns mutable reference to `Send` resource.
    ///
    /// # Panics
    ///
    /// This method will panic if resource is missing.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.expect_resource_mut::<i32>();
    /// ```
    ///
    /// ```
    /// # use edict::world::World;
    /// let mut world = World::new();
    /// world.insert_resource(42i32);
    /// *world.expect_resource_mut::<i32>() = 11;
    /// assert_eq!(*world.expect_resource_mut::<i32>(), 11);
    /// ```
    #[track_caller]
    pub fn expect_resource_mut<T: 'static>(&self) -> ResMut<T> {
        match unsafe { self.inner.resources.get_local_mut::<T>() } {
            Some(res) => res,
            None => panic!("Resource {} not found", type_name::<T>()),
        }
    }
}
