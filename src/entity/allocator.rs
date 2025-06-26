use alloc::boxed::Box;
use core::num::NonZeroU64;

/// Range of raw entity IDs.
/// `start` is inclusive, `end` is exclusive.
///
/// `IdRangeAllocator` provides ranges of IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IdRange {
    /// Start of the range. Inclusive.
    pub start: NonZeroU64,

    /// End of the range. Exclusive.
    pub end: NonZeroU64,
}

/// Start of the valid ID range.
pub const START: NonZeroU64 = NonZeroU64::new(1).unwrap();

/// End of the valid ID range.
/// This value is never allocated as valid ID.
pub const END: NonZeroU64 = NonZeroU64::new(u64::MAX).unwrap();

impl IdRange {
    /// Returns number of IDs in the range.
    pub fn count(&self) -> u64 {
        debug_assert!(self.start <= self.end);
        self.end.get() - self.start.get()
    }

    /// Returns true if the range is empty.
    pub fn is_empty(&self) -> bool {
        debug_assert!(self.start <= self.end);
        self.start == self.end
    }

    /// Returns ID at the given index.
    pub fn get(&self, idx: u64) -> Option<NonZeroU64> {
        if idx >= self.count() {
            return None;
        }

        // Safety: `self.start + idx` can't overflow
        // since `idx` is less than `self.count`.
        Some(unsafe { NonZeroU64::new_unchecked(self.start.get() + idx) })
    }

    /// Advances range by at most `count` IDs.
    /// Calls provided closure with each ID.
    /// Returns number of IDs advanced.
    pub fn advance(&mut self, count: u64, mut f: impl FnMut(NonZeroU64)) -> u64 {
        let count = count.min(self.count());

        let mut id = self.start;

        // Safety: `self.start + count` never overflows.
        self.start = unsafe { NonZeroU64::new_unchecked(self.start.get() + count) };

        while id < self.start {
            f(id);
            // Safety: `id + 1` never overflows
            // since it's less than another `NonZeroU64`.
            unsafe { id = NonZeroU64::new_unchecked(id.get() + 1) };
        }

        count
    }

    /// Take first ID from the range.
    pub fn take(&mut self) -> Option<NonZeroU64> {
        if self.is_empty() {
            return None;
        }

        let id = self.start;

        // Safety: `id + 1` can't overflow
        // since there's larger value.
        self.start = unsafe { NonZeroU64::new_unchecked(id.get() + 1) };

        Some(id)
    }
}

/// Returns proper range with `start` less than or equal to `end`.
fn proper_range(range: IdRange) -> IdRange {
    IdRange {
        start: range.start,
        end: range.end.max(range.start),
    }
}
pub(super) struct IdAllocator {
    current: IdRange,
    next: IdRange,
    range_alloc: Box<dyn IdRangeAllocator>,
}

impl IdAllocator {
    /// Id allocator that allocates IDs from [1..=u64::MAX].
    /// without external ID ranges.
    pub fn new() -> Self {
        IdAllocator {
            current: IdRange {
                start: START,
                end: END,
            },
            next: IdRange {
                start: END,
                end: END,
            },
            range_alloc: Box::new(DummyAllocator),
        }
    }

    /// Id allocator that allocates IDs from ranges.
    /// And allocate ranges from the given id range allocator.
    pub fn with_range_allocator(mut range_alloc: Box<dyn IdRangeAllocator>) -> Self {
        let current = range_alloc.allocate_range();
        let next = range_alloc.allocate_range();

        IdAllocator {
            current: proper_range(current),
            next: proper_range(next),
            range_alloc,
        }
    }

    /// Returns next ID from the range.
    /// If the range is exhausted, allocates new range from the allocator.
    /// If allocator is exhausted, returns `None`.
    pub fn next(&mut self) -> Option<NonZeroU64> {
        if self.current.is_empty() {
            self.current = self.next;
            self.next = proper_range(self.range_alloc.allocate_range());
        }

        self.current.take()
    }

    /// Reserves new ID.
    /// Call should use unique `idx` for each call
    /// between calls to `flush_reserved`.
    ///
    /// Caller SHOULD use `idx` values in order from 0 to not
    /// waste IDs.
    pub fn reserve(&self, idx: u64) -> Option<NonZeroU64> {
        if let Some(id) = self.current.get(idx) {
            return Some(id);
        }

        let idx2 = idx - self.current.count();
        self.next.get(idx2)
    }

    /// Returns reserve index of the ID.
    /// Returns `None` if ID is not reserved.
    pub fn reserved(&self, id: NonZeroU64) -> Option<u64> {
        let id = id.get();
        if id >= self.current.start.get() && id < self.current.end.get() {
            return Some(id - self.current.start.get());
        }
        if id >= self.next.start.get() && id < self.next.end.get() {
            return Some(id - self.next.start.get() + self.current.count());
        }
        None
    }

    /// Calls provided closure with reserved IDs.
    /// `count` must be larger than all `idx` values passed to `reserve` that
    /// returned `Some`
    #[inline]
    pub unsafe fn flush_reserved(&mut self, count: u64, mut f: impl FnMut(NonZeroU64)) {
        let mut advanced = self.current.advance(count, &mut f);
        if advanced < count {
            advanced += self.next.advance(count - advanced, &mut f);
            self.current = self.next;
            self.next = proper_range(self.range_alloc.allocate_range());
        }
        debug_assert_eq!(advanced, count);
    }
}

/// Allocator for entity IDs.
///
/// User may provide custom `IdRangeAllocator` implementation
/// to allocate ID ranges that `World` will be using.
///
/// This allows user to control IDs and ensure uniqueness across multiple worlds
/// when needed.
///
/// Allocator should return large range of IDs for two reasons.
/// First, it's faster to allocate IDs from pre-allocated range.
/// Second, entity reservation may not be able to allocate new range.
/// If current and pre-allocated ranges are exhausted, entity reservation will panic.
///
/// The actual size of range required to reserve entities between two flushes
/// is application specific, but `u32::MAX` is a safe upper bound
/// because edict does not support more than `u32::MAX` entities alive in the world.
///
/// # Safety
///
/// Implementation must allocate unique ID ranges.
pub unsafe trait IdRangeAllocator: Send + Sync + 'static {
    /// Allocate range of unique entity IDs.
    /// IDs generated must be unique for the given allocator.
    /// Special allocator types may enforce uniqueness
    /// across multiple allocator instances.\
    ///
    /// If allocator is exhausted, returns empty range.
    fn allocate_range(&mut self) -> IdRange;
}

struct DummyAllocator;

unsafe impl IdRangeAllocator for DummyAllocator {
    fn allocate_range(&mut self) -> IdRange {
        IdRange {
            start: END,
            end: END,
        }
    }
}

/// `IdRangeAllocator` implementation that allocates single ID range
/// provided in constructor.
pub struct OneRangeAllocator {
    range: IdRange,
}

const fn client_range() -> IdRange {
    IdRange {
        start: unsafe { NonZeroU64::new_unchecked(1) },
        end: unsafe { NonZeroU64::new_unchecked(1 << 48) },
    }
}

const fn server_range() -> IdRange {
    IdRange {
        start: unsafe { NonZeroU64::new_unchecked(1 << 48) },
        end: unsafe { NonZeroU64::new_unchecked(u64::MAX) },
    }
}

impl OneRangeAllocator {
    /// Creates new `OneRangeAllocator` that will allocate given range once.
    /// And then return empty range on subsequent allocations.
    pub const fn new(range: IdRange) -> Self {
        OneRangeAllocator { range }
    }

    /// Creates new `OneRangeAllocator` that will allocate
    /// client's entity ID range once.
    /// The client's ID range is pre-defined range `1..2^48`.
    ///
    /// The range is chosen to be large enough to not cause
    /// overflow in years of continuous client activity.
    pub const fn client() -> Self {
        OneRangeAllocator {
            range: client_range(),
        }
    }

    /// Creates new `OneRangeAllocator` that will allocate
    /// server's entity ID range once.
    /// The server's ID range is pre-defined range `2^48..2^64-1`.
    /// The range is chosen to be large enough to not cause
    /// overflow in thousands of years of continuous server activity.
    ///
    /// This allocator should only be used for isolated server setup.
    /// If servers are interconnected and share entities,
    /// construct custom allocator that will distribute ID ranges
    /// from common pool.
    pub const fn server() -> Self {
        OneRangeAllocator {
            range: server_range(),
        }
    }
}

unsafe impl IdRangeAllocator for OneRangeAllocator {
    fn allocate_range(&mut self) -> IdRange {
        let range = self.range;
        self.range.start = self.range.end;
        range
    }
}
