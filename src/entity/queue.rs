use core::{
    alloc::Layout,
    cell::UnsafeCell,
    ptr::{self, NonNull},
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::vec::Vec;

#[derive(PartialEq, Eq)]
pub(crate) struct DropQueue {
    inner: NonNull<DropQueueInner<[UnsafeCell<u32>]>>,
}

/// # Safety
///
/// This is basically `Arc<DropQueueInner>` without weak references
/// and with known layout.
///
/// Access is synchronized with atomic locks.
unsafe impl Send for DropQueue {}

/// # Safety
///
/// This is basically `Arc<DropQueueInner>` without weak references
/// and with known layout.
///
/// Access is synchronized with atomic locks.
unsafe impl Sync for DropQueue {}

///
/// !!!WARNING!!!
/// `DropQueue::new` relies on this type layout.
/// Any changes in the fields, their types or order must be reflected in that function.
///
#[repr(C)]
struct DropQueueInner<Q: ?Sized> {
    refs: AtomicUsize,
    lock: AtomicUsize,
    cursor: AtomicUsize,
    tail: UnsafeCell<Vec<u32>>,
    buffer: Q,
}

impl Drop for DropQueue {
    fn drop(&mut self) {
        let inner = self.inner.as_ptr();
        let old = unsafe { (*inner).refs.fetch_sub(1, Ordering::Release) };

        if old == 1 {
            unsafe {
                ptr::drop_in_place(inner);
                let layout = Layout::for_value(&*inner);
                alloc::alloc::dealloc(inner.cast(), layout)
            };
        }
    }
}

impl Clone for DropQueue {
    fn clone(&self) -> Self {
        let inner = self.inner.as_ptr();
        unsafe { (*inner).refs.fetch_add(1, Ordering::Relaxed) };
        DropQueue { inner: self.inner }
    }
}

impl DropQueue {
    pub fn new(inline_cap: usize) -> Self {
        //
        // !!!WARNING!!!
        // This code relies on layout of the `DropQueueInner` type.
        // Any changes in the fields, their types or order must be reflected here.
        //
        let atomic = Layout::new::<AtomicUsize>();
        let tail = Layout::new::<UnsafeCell<Vec<u32>>>();
        let buffer = match Layout::array::<UnsafeCell<u32>>(inline_cap) {
            Err(err) => {
                panic!(
                    "Failed to allocate Queue with inline capacity: '{}'. {:#}",
                    inline_cap, err
                );
            }
            Ok(layout) => layout,
        };

        let refs_offset = 0;
        let (queue_inner, lock_offset) = atomic.extend(atomic).unwrap();
        let (queue_inner, cursor_offset) = queue_inner.extend(atomic).unwrap();
        let (queue_inner, tail_offset) = queue_inner.extend(tail).unwrap();

        match queue_inner.extend(buffer) {
            Err(err) => {
                panic!(
                    "Failed to allocate Queue with inline capacity: '{}'. {:#}",
                    inline_cap, err
                );
            }
            Ok((queue_inner, buffer_offset)) => {
                let queue_inner = queue_inner.pad_to_align();

                unsafe {
                    let ptr = alloc::alloc::alloc(queue_inner);

                    ptr::write(ptr.add(refs_offset) as _, AtomicUsize::new(1));
                    ptr::write(ptr.add(lock_offset) as _, AtomicUsize::new(0));
                    ptr::write(ptr.add(cursor_offset) as _, AtomicUsize::new(0));
                    ptr::write(
                        ptr.add(tail_offset) as _,
                        UnsafeCell::new(Vec::<u32>::new()),
                    );
                    ptr::write_bytes(
                        ptr.add(buffer_offset) as *mut UnsafeCell<u32>,
                        0,
                        inline_cap,
                    );

                    // This line relies on rust representation.
                    let fat_ptr = ptr::slice_from_raw_parts_mut(ptr, inline_cap) as _;

                    DropQueue {
                        inner: NonNull::new(fat_ptr).unwrap(),
                    }
                }
            }
        }
    }

    pub fn drop_entity(&self, id: u32) {
        unsafe { &*self.inner.as_ptr() }.drop_entity(id);
    }

    pub fn drain<'a>(&'a self, extend: &mut Vec<u32>) {
        unsafe { &*self.inner.as_ptr() }.drain(extend)
    }
}

impl DropQueueInner<[UnsafeCell<u32>]> {
    fn drop_entity(&self, id: u32) {
        if self.buffer.len() == 0 {
            loop {
                let res = self.lock.compare_exchange_weak(
                    0,
                    isize::MAX as usize,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                );

                match res {
                    Ok(_) => unsafe {
                        (*self.tail.get()).push(id);
                        self.lock.store(0, Ordering::Release);
                        return;
                    },
                    Err(_) => {
                        yield_now();
                    }
                }
            }
        }

        loop {
            let locked = self.lock.fetch_add(1, Ordering::Acquire);

            if locked >= isize::MAX as usize {
                // Exclusive lock was acquired elsewhere.
                self.lock.fetch_sub(1, Ordering::Release);
                yield_now();
                continue;
            }

            let idx = self.cursor.fetch_add(1, Ordering::Relaxed);
            if idx < self.buffer.len() {
                unsafe {
                    *self.buffer[idx].get() = id;
                }
                self.lock.fetch_sub(1, Ordering::Release);
                return;
            } else if idx == self.buffer.len() {
                loop {
                    let res = self.lock.compare_exchange_weak(
                        1,
                        isize::MAX as usize,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    );

                    match res {
                        Ok(_) => {
                            // With exclusive lock

                            let tail = unsafe { &mut *self.tail.get() };
                            tail.reserve(self.buffer.len() + 1);
                            tail.push(id);
                            tail.extend(self.buffer.iter().map(|c| unsafe { *c.get() }));

                            self.cursor.store(0, Ordering::Relaxed);

                            self.lock.fetch_sub(isize::MAX as usize, Ordering::Release);
                            return;
                        }
                        Err(_) => yield_now(),
                    }
                }
            } else {
                // Buffer exhausted.
                self.lock.fetch_sub(1, Ordering::Release);
                yield_now();
            }
        }
    }

    fn drain<'a>(&'a self, extend: &mut Vec<u32>) {
        loop {
            let res = self.lock.compare_exchange_weak(
                0,
                isize::MAX as usize,
                Ordering::Acquire,
                Ordering::Relaxed,
            );

            match res {
                Ok(_) => {
                    let len = self.cursor.swap(0, Ordering::Relaxed);

                    extend.extend(self.buffer[..len].iter().map(|id| unsafe { *id.get() }));
                    extend.append(unsafe { &mut *self.tail.get() });

                    self.lock.fetch_sub(isize::MAX as usize, Ordering::Release);
                    break;
                }
                Err(_) => yield_now(),
            }
        }
    }
}

fn yield_now() {
    #[cfg(feture = "std")]
    {
        std::thread::yield_now();
    }
}
