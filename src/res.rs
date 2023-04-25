//! Provides `Res` - a type-map for singleton type values called "resources".
//!
//! `Res` supports both `Send`, `Sync` and also `!Send`, `!Sync` resource types.
//! To be sound `Res` intentionally does not implement `Send`. The thread where `Res` is created is considered "main" thread,
//! but it doesn't have to be main thread of the process.
//!
//! `Res` is still usable in multithreaded programs as it implements `Sync`
//! and uses type bounds on API to ensure that immutable reference to `!Sync`
//! and mutable references to `!Send` types is not obtainable from outside "main" thread.
//!
//! Internally `Res` acts as collection of thread-safe `RefCell`s, so an attempt to fetch mutable reference
//! to the same value twice or mutable and immutable references at the same time is prohibited and cause panics.
//!

use alloc::boxed::Box;
use core::{
    any::{type_name, Any, TypeId},
    fmt::{self, Debug},
};

use atomicell::{AtomicCell, Ref, RefMut};
use hashbrown::HashMap;

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
pub struct Res {
    resources: HashMap<TypeId, Resource>,
}

impl Debug for Res {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.resources)
    }
}

impl Res {
    /// Returns a new empty resource container.
    #[inline]
    pub fn new() -> Self {
        Res {
            resources: HashMap::new(),
        }
    }

    /// Inserts resource into container.
    /// Old value is replaced.
    ///
    /// For `Res<Send>` resource type have to implement `Send`.
    /// Allowing `Res<Send>` to be moved into another thread and drop resources there.
    ///
    /// `Res<NoSend>` accepts any `'static` resource type.
    pub fn insert<T: 'static>(&mut self, resource: T) {
        let id = TypeId::of::<T>();
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
    /// For `Res<Send>` resource type have to implement `Send`.
    /// Allowing `Res<Send>` to be moved into another thread and drop resources there.
    ///
    /// `Res<NoSend>` accepts any `'static` resource type.
    pub fn with<T: 'static>(&mut self, f: impl FnOnce() -> T) -> &mut T {
        let id = TypeId::of::<T>();
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
        let resource = self.resources.remove(&TypeId::of::<T>())?;
        let value = AtomicCell::into_inner(*unsafe {
            // # Safety
            // Manually casting is is safe, type behind `dyn Any` is `T`.
            Box::from_raw(Box::into_raw(resource.data) as *mut AtomicCell<T>)
        });
        Some(value)
    }

    /// Returns some reference to `Sync` resource.
    /// Returns none if resource is not found.
    #[inline]
    pub fn get<T: Sync + 'static>(&self) -> Option<Ref<T>> {
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
    pub fn get_mut<T: Send + 'static>(&self) -> Option<RefMut<T>> {
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
    pub unsafe fn get_local<T: 'static>(&self) -> Option<Ref<T>> {
        let id = TypeId::of::<T>();

        let r = {
            // # Safety
            // Index from `ids` always valid.
            &*self.resources.get(&id)?.data
        }
        .borrow();

        let r = Ref::map(r, |r| r.downcast_ref::<T>().unwrap());
        Some(r)
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
    pub unsafe fn get_local_mut<T: 'static>(&self) -> Option<RefMut<T>> {
        let id = TypeId::of::<T>();

        let r = {
            // # Safety
            // Index from `ids` always valid.
            &*self.resources.get(&id)?.data
        }
        .borrow_mut();

        let r = RefMut::map(r, |r| r.downcast_mut::<T>().unwrap());
        Some(r)
    }

    /// Reset all possible leaks on resources.
    /// Mutable reference guarantees that no borrows are active.
    pub fn undo_leak(&mut self) {
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
