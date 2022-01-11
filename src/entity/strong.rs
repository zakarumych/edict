use core::{
    alloc::Layout,
    cell::UnsafeCell,
    fmt,
    ptr::{drop_in_place, slice_from_raw_parts_mut, write, write_bytes, NonNull},
    slice,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::vec::{self, Vec};

use super::weak::WeakEntity;

#[derive(PartialEq, Eq)]
pub(super) struct StrongEntity {
    pub weak: WeakEntity,
    inner: NonNull<StrongEntityInner>,
}

struct StrongEntityInner {
    refs: AtomicUsize,
    queue: DropQueue,
}

impl Drop for StrongEntity {
    fn drop(&mut self) {
        let old = unsafe { (*self.inner.as_ptr()).refs.fetch_sub(1, Ordering::Release) };
        if old == 1 {
            unsafe {
                (*self.inner.as_ptr()).queue.send(self.weak.id);
            }
            unsafe {
                drop_in_place(self.inner.as_ptr());
            }
            unsafe {
                alloc::alloc::dealloc(
                    self.inner.as_ptr().cast(),
                    Layout::new::<StrongEntityInner>(),
                );
            }
        }
    }
}

impl Clone for StrongEntity {
    fn clone(&self) -> Self {
        unsafe {
            (*self.inner.as_ptr()).refs.fetch_add(1, Ordering::Relaxed);
        }
        StrongEntity {
            weak: self.weak,
            inner: self.inner,
        }
    }
}

impl StrongEntity {
    pub fn new(weak: WeakEntity, queue: &DropQueue) -> Self {
        let ptr = unsafe { alloc::alloc::alloc(Layout::new::<StrongEntityInner>()).cast() };
        let inner = StrongEntityInner {
            refs: AtomicUsize::new(1),
            queue: queue.clone(),
        };
        unsafe {
            write(ptr, inner);
        }
        StrongEntity {
            weak,
            inner: NonNull::new(ptr).unwrap(),
        }
    }
}

pub(crate) struct DropQueue {
    inner: NonNull<QueueInner<[UnsafeCell<u32>]>>,
}

impl Drop for DropQueue {
    fn drop(&mut self) {
        let old = unsafe { (*self.inner.as_ptr()).refs.fetch_sub(1, Ordering::Release) };

        if old == 1 {
            let layout = unsafe { Layout::for_value(&*self.inner.as_ptr()) };
            unsafe { alloc::alloc::dealloc(self.inner.as_ptr().cast(), layout) };
        }
    }
}

impl Clone for DropQueue {
    fn clone(&self) -> Self {
        unsafe {
            (*self.inner.as_ptr()).refs.fetch_sub(1, Ordering::Relaxed);
        }
        DropQueue { inner: self.inner }
    }
}

impl fmt::Debug for DropQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Queue").finish_non_exhaustive()
    }
}

impl DropQueue {
    pub fn new(inline_cap: usize) -> Self {
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

                    write(ptr.add(refs_offset) as _, AtomicUsize::new(1));
                    write(ptr.add(lock_offset) as _, AtomicUsize::new(0));
                    write(ptr.add(cursor_offset) as _, AtomicUsize::new(0));
                    write(
                        ptr.add(tail_offset) as _,
                        UnsafeCell::new(Vec::<u32>::new()),
                    );
                    write_bytes(
                        ptr.add(buffer_offset) as *mut UnsafeCell<u32>,
                        0,
                        inline_cap,
                    );

                    let slice_ptr = slice_from_raw_parts_mut(ptr, inline_cap) as _;

                    DropQueue {
                        inner: NonNull::new(slice_ptr).unwrap(),
                    }
                }
            }
        }
    }

    pub fn drain(&mut self) -> impl Iterator<Item = u32> + '_ {
        unsafe { (*self.inner.as_ptr()).drain() }
    }

    fn send(&self, id: u32) {
        unsafe {
            (*self.inner.as_ptr()).send(id);
        }
    }
}

#[repr(C)]
struct QueueInner<Q: ?Sized> {
    refs: AtomicUsize,
    lock: AtomicUsize,
    cursor: AtomicUsize,
    tail: UnsafeCell<Vec<u32>>,
    buffer: Q,
}

impl<Q> QueueInner<Q>
where
    Q: AsRef<[UnsafeCell<u32>]> + ?Sized,
{
    fn send(&self, id: u32) {
        if self.buffer.as_ref().len() == 0 {
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

            let buffer = self.buffer.as_ref();

            let idx = self.cursor.fetch_add(1, Ordering::Relaxed);
            if idx < buffer.len() {
                unsafe {
                    *buffer[idx].get() = id;
                }
                self.lock.fetch_sub(1, Ordering::Release);
                return;
            } else if idx == buffer.len() {
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
                            tail.reserve(buffer.len() + 1);
                            tail.push(id);
                            tail.extend(buffer.iter().map(|c| unsafe { *c.get() }));

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

    fn drain(&self) -> impl Iterator<Item = u32> + '_ {
        loop {
            let res = self.lock.compare_exchange_weak(
                0,
                isize::MAX as usize,
                Ordering::Acquire,
                Ordering::Relaxed,
            );

            match res {
                Ok(_) => {
                    let len = self.cursor.load(Ordering::Relaxed);
                    let ids = self.buffer.as_ref()[..len].iter();
                    let tail = unsafe { &mut *self.tail.get() }.drain(..);

                    return QueueDrain {
                        ids,
                        tail,
                        unlock: &self.lock,
                    };
                }
                Err(_) => yield_now(),
            }
        }
    }
}

struct QueueDrain<'a> {
    ids: slice::Iter<'a, UnsafeCell<u32>>,
    tail: vec::Drain<'a, u32>,
    unlock: &'a AtomicUsize,
}

impl Drop for QueueDrain<'_> {
    fn drop(&mut self) {
        self.unlock
            .fetch_sub(isize::MAX as usize, Ordering::Release);
    }
}

impl Iterator for QueueDrain<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        self.ids
            .next()
            .map(|id| unsafe { *id.get() })
            .or_else(|| self.tail.next())
    }
}

const QUEUE_LEN: usize = 1024;

fn yield_now() {
    #[cfg(feture = "std")]
    {
        std::thread::yield_now();
    }
}
