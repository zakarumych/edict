use core::{num::NonZeroU64, ops::Range};

pub struct IdRange {
    start: NonZeroU64,
    count: u32,
}

impl IdRange {
    /// # Safety
    ///
    /// `start + count` must not overflow `u64`.
    unsafe fn new(start: NonZeroU64, count: u32) -> Self {
        Self { start, count }
    }

    pub fn range(&self) -> Range<u64> {
        self.start.get()..self.start.get() + u64::from(self.count)
    }

    pub fn next(&mut self, alloc: &mut dyn IdAllocator) -> Option<NonZeroU64> {
        if self.count == 0 {
            *self = alloc.allocate_range();
        }

        if self.count == 0 {
            return None;
        }

        let id = self.start;

        // Safety: `self.start + self.count` never overflows.
        self.start = unsafe { NonZeroU64::new_unchecked(self.start.get() + 1) };
        self.count -= 1;

        Some(id)
    }

    pub fn get(&self, idx: u32) -> Option<NonZeroU64> {
        if self.count <= idx {
            return None;
        }

        // Safety: `self.start + idx` never overflows.
        let id = unsafe { NonZeroU64::new_unchecked(self.start.get() + u64::from(idx)) };

        Some(id)
    }

    pub unsafe fn advance(&mut self, count: u32, mut f: impl FnMut(NonZeroU64)) {
        debug_assert!(count <= self.count);

        let mut id = self.start;
        // Safety: `self.start + count` never overflows.
        self.start = NonZeroU64::new_unchecked(self.start.get() + u64::from(count));

        while id.get() < self.start.get() {
            f(id);
            // Safety: `id + 1` never overflows
            // since it's less than another `NonZeroU64`.
            unsafe { id = NonZeroU64::new_unchecked(id.get() + 1) };
        }
    }
}

/// Allocator for entity IDs.
pub unsafe trait IdAllocator {
    /// Allocate range of unique entity IDs.
    /// IDs generated are unique for the given allocator.
    /// Special allocator types may enforce uniqueness across allocator instances.
    /// Returns actual number of IDs allocated.
    /// If allocator is exhausted, returns `(_, 0)`.
    fn allocate_range(&mut self) -> IdRange;
}

pub struct CursorAllocator {
    cursor: u64,
}

impl CursorAllocator {
    pub fn new() -> Self {
        CursorAllocator { cursor: 2 }
    }
}

unsafe impl IdAllocator for CursorAllocator {
    fn allocate_range(&mut self) -> IdRange {
        let count = u64::from(u32::MAX);
        let start = self.cursor;
        let end = start.saturating_add(count);
        self.cursor = end;
        let allocated = end - start;
        let allocated = allocated as u32;

        // Safety: cursor stars with 1 and never wraps.
        let id = unsafe { NonZeroU64::new_unchecked(start) };

        // Safety: `allocated = end - start` where `end >= start`
        // hence `start + allocated` never overflows.
        unsafe { IdRange::new(id, allocated) }
    }
}
