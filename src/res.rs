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

use core::{
    any::{type_name, Any, TypeId},
    fmt::{self, Debug},
};

use atomicell::{AtomicCell, Ref, RefMut};

use crate::typeidset::TypeIdSet;

struct Resource {
    // Box<AtomicCell> instead of AtomicCell<Box> to avoid false sharing
    data: Box<AtomicCell<dyn Any>>,
    type_name: &'static str,
}

impl Debug for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.type_name)
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
    ids: TypeIdSet,
    resources: Vec<Resource>,
}

impl Debug for Res {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.resources)
    }
}

impl Res {
    /// Returns a new empty resource container.
    pub fn new() -> Self {
        Res {
            ids: TypeIdSet::new(core::iter::empty()),
            resources: Vec::new(),
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
        let id = resource.type_id();

        match self.ids.get(id) {
            None => {
                self.ids = TypeIdSet::new(
                    self.resources
                        .iter()
                        .map(|r| r.data.type_id())
                        .chain(core::iter::once(id)),
                );

                self.resources.push(Resource {
                    data: Box::new(AtomicCell::new(resource)),
                    type_name: type_name::<T>(),
                });
            }
            Some(idx) => {
                let data = unsafe {
                    // # Safety
                    // Index from `ids` always valid.
                    self.resources.get_unchecked_mut(idx)
                }
                .data
                .get_mut();

                *data.downcast_mut::<T>().unwrap() = resource;
            }
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
    /// If `T` is `Sync` then this method is also safe.
    /// In this case prefer to use [`get`] method instead.
    pub unsafe fn get_local<T: 'static>(&self) -> Option<Ref<T>> {
        let idx = self.ids.get(TypeId::of::<T>())?;

        let r = {
            // # Safety
            // Index from `ids` always valid.
            &*self.resources.get_unchecked(idx).data
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
    /// If `T` is `Send` then this method is also safe.
    /// In this case prefer to use [`get_mut`] method instead.
    pub unsafe fn get_local_mut<T: 'static>(&self) -> Option<RefMut<T>> {
        let idx = self.ids.get(TypeId::of::<T>())?;

        let r = {
            // # Safety
            // Index from `ids` always valid.
            &*self.resources.get_unchecked(idx).data
        }
        .borrow_mut();

        let r = RefMut::map(r, |r| r.downcast_mut::<T>().unwrap());
        Some(r)
    }

    /// Returns some reference to `Sync` resource.
    /// Returns none if resource is not found.
    pub fn get<T: Sync + 'static>(&self) -> Option<Ref<T>> {
        let idx = self.ids.get(TypeId::of::<T>())?;

        let r = unsafe {
            // # Safety
            // Index from `ids` always valid.
            &*self.resources.get_unchecked(idx).data
        }
        .borrow();

        let r = Ref::map(r, |r| r.downcast_ref::<T>().unwrap());
        Some(r)
    }

    /// Returns some mutable reference to `Send` resource.
    /// Returns none if resource is not found.
    pub fn get_mut<T: Send + 'static>(&self) -> Option<RefMut<T>> {
        let idx = self.ids.get(TypeId::of::<T>())?;

        let r = unsafe {
            // # Safety
            // Index from `ids` always valid.
            &*self.resources.get_unchecked(idx).data
        }
        .borrow_mut();
        let r = RefMut::map(r, |r| r.downcast_mut::<T>().unwrap());
        Some(r)
    }
}
