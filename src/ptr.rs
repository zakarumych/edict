//! Contains ptr wrappers.

use core::{alloc::Layout, any::TypeId, marker::PhantomData, mem::size_of, ptr::NonNull};

#[cfg(debug_assertions)]
mod debug {
    use core::{alloc::Layout, any::TypeId};

    #[derive(Clone, Copy)]
    pub enum Kind {
        Mutable,
        Immutable,
        Dangling,
    }

    pub struct Meta {
        id: TypeId,
        layout: Layout,
        count: usize,
        kind: Kind,
    }

    pub trait GetMeta {
        fn get(self) -> Meta;
    }

    impl<T> GetMeta for &T
    where
        T: 'static,
    {
        #[inline]
        fn get(self) -> Meta {
            Meta {
                id: TypeId::of::<T>(),
                layout: Layout::new::<T>(),
                count: 1,
                kind: Kind::Immutable,
            }
        }
    }

    impl<T> GetMeta for &mut T
    where
        T: 'static,
    {
        #[inline]
        fn get(self) -> Meta {
            Meta {
                id: TypeId::of::<T>(),
                layout: Layout::new::<T>(),
                count: 1,
                kind: Kind::Mutable,
            }
        }
    }

    impl<T> GetMeta for &[T]
    where
        T: 'static,
    {
        #[inline]
        fn get(self) -> Meta {
            Meta {
                id: TypeId::of::<T>(),
                layout: Layout::new::<T>(),
                count: self.len(),
                kind: Kind::Immutable,
            }
        }
    }

    impl<T> GetMeta for &mut [T]
    where
        T: 'static,
    {
        #[inline]
        fn get(self) -> Meta {
            Meta {
                id: TypeId::of::<T>(),
                layout: Layout::new::<T>(),
                count: self.len(),
                kind: Kind::Mutable,
            }
        }
    }

    impl Meta {
        #[inline]
        pub fn new(id: TypeId, layout: Layout, count: usize, kind: Kind) -> Self {
            Meta {
                id,
                layout,
                count,
                kind,
            }
        }

        #[inline]
        pub fn from<T: GetMeta>(t: T) -> Self {
            t.get()
        }

        #[inline]
        pub fn dangling<T: 'static>() -> Self {
            Meta {
                id: TypeId::of::<T>(),
                layout: Layout::new::<T>(),
                count: 0,
                kind: Kind::Dangling,
            }
        }

        #[inline]
        pub fn is_valid<T: ?Sized + 'static>(&self) -> bool {
            matches!(self.kind, Kind::Immutable | Kind::Mutable) && self.id == TypeId::of::<T>()
        }

        #[inline]
        pub fn is_valid_mut<T: ?Sized + 'static>(&self) -> bool {
            matches!(self.kind, Kind::Mutable) && self.id == TypeId::of::<T>()
        }

        #[inline]
        pub fn is_mut(&self) -> bool {
            matches!(self.kind, Kind::Mutable)
        }

        #[inline]
        pub fn layout(&self) -> Layout {
            self.layout
        }

        #[inline]
        pub fn add(&self, offset: usize) -> Self {
            assert_eq!(offset % self.layout.size(), 0);
            assert_eq!(offset % self.layout.align(), 0);

            let offset = offset / self.layout.size();
            assert!(offset <= self.count);

            Meta {
                id: self.id,
                layout: self.layout,
                count: self.count - offset,
                kind: self.kind,
            }
        }

        #[inline]
        pub fn borrow(&self) -> Self {
            Meta {
                id: self.id,
                layout: self.layout,
                count: self.count,
                kind: match self.kind {
                    Kind::Mutable | Kind::Immutable => Kind::Immutable,
                    Kind::Dangling => Kind::Dangling,
                },
            }
        }

        #[inline]
        pub fn borrow_mut(&self) -> Self {
            Meta {
                id: self.id,
                layout: self.layout,
                count: self.count,
                kind: self.kind,
            }
        }
    }
}

/// NonNullRef pointer with specific lifetime.
/// With debug assertions enabled, it asserts type, layout and mutability.
#[cfg_attr(debug_assertions, repr(C))]
#[cfg_attr(not(debug_assertions), repr(transparent))]
pub struct NonNullRef<'a, T: ?Sized = u8> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a ()>,

    #[cfg(debug_assertions)]
    debug: debug::Meta,
}

impl<'a, T> NonNullRef<'a, T>
where
    T: 'static,
{
    #[inline]
    pub fn from_ref(r: &'a T) -> Self {
        NonNullRef {
            #[cfg(debug_assertions)]
            debug: debug::Meta::from(r),
            ptr: NonNull::from(r),
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn from_mut(r: &'a mut T) -> Self {
        NonNullRef {
            #[cfg(debug_assertions)]
            debug: debug::Meta::from(&mut *r),
            ptr: NonNull::from(r),
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn from_ptr(ptr: NonNull<T>, len: usize, mutable: bool) -> Self {
        NonNullRef {
            #[cfg(debug_assertions)]
            debug: debug::Meta::new(
                TypeId::of::<T>(),
                Layout::new::<T>(),
                len,
                if mutable {
                    debug::Kind::Mutable
                } else {
                    debug::Kind::Immutable
                },
            ),
            ptr,
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn dangling() -> Self {
        NonNullRef {
            #[cfg(debug_assertions)]
            debug: debug::Meta::dangling::<T>(),
            ptr: NonNull::<T>::dangling().cast(),
            marker: PhantomData,
        }
    }
}

impl<'a, T> NonNullRef<'a, T> {
    #[inline]
    pub fn raw_dangling(id: TypeId, layout: Layout) -> Self {
        NonNullRef {
            #[cfg(debug_assertions)]
            debug: debug::Meta::new(id, layout, 0, debug::Kind::Dangling),
            ptr: unsafe { NonNull::new_unchecked(layout.align() as _) },
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn add(&self, offset: usize) -> Self {
        debug_assert_eq!(offset % size_of::<T>(), 0);
        debug_assert!((self.ptr.as_ptr() as usize).checked_add(offset).is_some());

        let ptr = unsafe { NonNull::new_unchecked(self.ptr.as_ptr().add(offset / size_of::<T>())) };

        NonNullRef {
            #[cfg(debug_assertions)]
            debug: self.debug.add(offset),
            ptr,
            marker: PhantomData,
        }
    }

    pub fn cast<U>(self) -> NonNullRef<'a, U> {
        NonNullRef {
            #[cfg(debug_assertions)]
            debug: self.debug,
            ptr: self.ptr.cast(),
            marker: PhantomData,
        }
    }
}

impl<'a, T> NonNullRef<'a, T>
where
    T: ?Sized + 'static,
{
    #[inline]
    pub unsafe fn as_ref(&self) -> &T {
        debug_assert!(self.debug.is_valid::<T>());
        self.ptr.as_ref()
    }

    #[inline]
    pub unsafe fn as_mut(&mut self) -> &mut T {
        debug_assert!(self.debug.is_valid_mut::<T>());
        self.ptr.as_mut()
    }

    #[inline]
    pub unsafe fn into_ref(self) -> &'a T {
        debug_assert!(self.debug.is_valid::<T>());
        self.ptr.as_ref()
    }

    #[inline]
    pub unsafe fn into_mut(mut self) -> &'a mut T {
        debug_assert!(self.debug.is_valid_mut::<T>());
        self.ptr.as_mut()
    }

    #[inline]
    pub fn from_raw(
        ptr: NonNull<T>,
        id: TypeId,
        layout: Layout,
        count: usize,
        mutable: bool,
    ) -> Self {
        debug_assert_eq!((ptr.as_ptr() as *mut u8 as usize) % layout.align(), 0);

        NonNullRef {
            #[cfg(debug_assertions)]
            debug: debug::Meta::new(
                id,
                layout,
                count,
                if mutable {
                    debug::Kind::Mutable
                } else {
                    debug::Kind::Immutable
                },
            ),
            ptr,
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr()
    }

    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        debug_assert!(self.debug.is_mut());
        self.ptr.as_ptr()
    }

    #[inline]
    pub fn borrow(&self) -> NonNullRef<T> {
        NonNullRef {
            #[cfg(debug_assertions)]
            debug: self.debug.borrow(),
            ptr: self.ptr,
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn borrow_mut(&self) -> NonNullRef<T> {
        NonNullRef {
            #[cfg(debug_assertions)]
            debug: self.debug.borrow_mut(),
            ptr: self.ptr,
            marker: PhantomData,
        }
    }
}
