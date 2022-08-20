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
    cell::Cell,
    fmt::{self, Debug},
    marker::PhantomData,
};

use atomicell::{AtomicCell, Ref, RefMut};

use crate::typeidset::TypeIdSet;

struct Resource {
    // Box<AtomicCell> instead of AtomicCell<Box> to avoid false sharing
    data: Box<AtomicCell<dyn Any>>,
    type_name: &'static str,
}

/// Type-erased container for singleton resources.
///
/// # Examples
///
/// `Res` intentionally doesn't implement `Send`
/// yet implements `Sync`.
///
/// ```compile_fail
/// # use edict::res::Res;
///
/// fn test_send<T: Send>() {}
/// test_send::<Res>();
/// ```
///
/// ```
/// # use edict::res::Res;
///
/// fn test_sync<T: Sync>() {}
/// test_sync::<Res>();
/// ```
pub struct Res {
    ids: TypeIdSet,
    resources: Vec<Resource>,
}

impl Debug for Res {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut iter = self.resources.iter();

        if let Some(first) = iter.next() {
            f.write_str("Res {")?;
            write!(f, "{}", first.type_name)?;

            for resource in iter {
                write!(f, ", {}", resource.type_name,)?;
            }

            f.write_str("}")
        } else {
            f.write_str("Res {}")
        }
    }
}

/// A reference to [`Res`] that allows to fetch local resources.
///
/// # Examples
///
/// `ResLocal` intentionally doesn't implement `Send` or `Sync`.
///
/// ```compile_fail
/// # use edict::res::ResLocal;
///
/// fn test_send<T: Send>() {}
/// test_send::<ResLocal>();
/// ```
///
/// ```compile_fail
/// # use edict::res::ResLocal;
///
/// fn test_sync<T: Sync>() {}
/// test_sync::<ResLocal>();
/// ```
pub struct ResLocal<'a> {
    res: &'a mut Res,
    marker: PhantomData<Cell<Res>>,
}

impl Debug for ResLocal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut iter = self.res.resources.iter();

        if let Some(first) = iter.next() {
            f.write_str("ResLocal {")?;
            write!(f, "{}", first.type_name)?;

            for resource in iter {
                write!(f, ", {}", resource.type_name,)?;
            }

            f.write_str("}")
        } else {
            f.write_str("ResLocal {}")
        }
    }
}

// # Safety
// Event though `Res` can contain `!Send` or `!Sync` resources,
// `get_local` method is marked unsafe and caller is responsible to ensure that
// it is safe to call.
//
// And `ResLocal` wrapper contains `&mut Res` to guarantee that it exists only on the same thread
// where `Res` is created and resources are inserted.
unsafe impl Sync for Res {}

// `Res` is not `Send` to disallow placing to another thread.

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

    /// Returns [`ResLocal`] referencing this `Res`.
    /// [`ResLocal`] provides same API but allows to fetch local resources safely.
    pub fn local(&mut self) -> ResLocal<'_> {
        ResLocal {
            res: self,
            marker: PhantomData,
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

impl ResLocal<'_> {
    /// Inserts resource into container.
    /// Old value is replaced.
    pub fn insert<T: 'static>(&mut self, resource: T) {
        self.res.insert(resource)
    }

    /// Returns some reference to a resource.
    /// Returns none if resource is not found.
    pub fn get<T: 'static>(&self) -> Option<Ref<T>> {
        unsafe {
            // # Safety
            // Mutable reference to `Res` ensures this is the "main" thread.
            self.res.get_local()
        }
    }

    /// Returns some mutable reference to a resource.
    /// Returns none if resource is not found.
    pub fn get_mut<T: 'static>(&self) -> Option<RefMut<T>> {
        unsafe {
            // # Safety
            // Mutable reference to `Res` ensures this is the "main" thread.
            self.res.get_local_mut()
        }
    }
}
