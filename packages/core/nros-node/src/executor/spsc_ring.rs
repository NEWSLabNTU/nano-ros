//! Lock-free SPSC ring buffer for `KEEP_LAST(N)` subscriptions (N > 1).
//!
//! A fixed-capacity ring with atomic head/tail indices over a pre-allocated
//! region. Uses N+1 slots for full/empty disambiguation (Lamport's trick).
//!
//! The ring does not own its memory — it operates on a region provided by
//! the arena. Per-slot data lengths are stored in a trailing `[usize]` array
//! immediately after the slot data region.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Lock-free SPSC ring buffer over a pre-allocated memory region.
///
/// # Layout
///
/// The caller provides a contiguous region large enough for:
/// - `capacity * slot_size` bytes for data slots
/// - `capacity * size_of::<usize>()` bytes for per-slot length tracking
///
/// Use [`SpscRing::region_size`] to compute the total required bytes.
///
/// # Thread safety
///
/// Safe for single-producer single-consumer (one writer, one reader).
pub(crate) struct SpscRing {
    buf_ptr: *mut u8,
    slot_size: usize,
    /// Number of slots (depth + 1 for full/empty disambiguation).
    capacity: usize,
    /// Pointer to per-slot length array (trailing region after data slots).
    lengths_ptr: *mut usize,
    /// Writer position (next slot to write into).
    head: AtomicUsize,
    /// Reader position (next slot to read from).
    tail: AtomicUsize,
}

// Safety: SPSC usage — one writer (network callback), one reader (executor).
unsafe impl Send for SpscRing {}
unsafe impl Sync for SpscRing {}

impl SpscRing {
    /// Compute the total region size needed for a ring with the given depth
    /// and slot size.
    ///
    /// `depth` is the user-facing QoS depth. Internally, `capacity = depth + 1`
    /// slots are allocated (one extra for full/empty disambiguation).
    pub const fn region_size(depth: usize, slot_size: usize) -> usize {
        let capacity = depth + 1;
        capacity * slot_size + capacity * core::mem::size_of::<usize>()
    }

    /// Slot count including the extra disambiguation slot.
    pub const fn slot_count(depth: usize) -> usize {
        depth + 1
    }

    /// Initialize a ring buffer over a pre-allocated region.
    ///
    /// `depth` is the user-facing QoS depth (e.g., 5 for `KEEP_LAST(5)`).
    ///
    /// # Safety
    ///
    /// `buf_ptr` must point to a region of at least
    /// [`region_size(depth, slot_size)`](Self::region_size) writable bytes
    /// that remains valid for the lifetime of this `SpscRing`.
    pub unsafe fn init(buf_ptr: *mut u8, slot_size: usize, depth: usize) -> Self {
        let capacity = depth + 1;
        let lengths_ptr = unsafe { buf_ptr.add(capacity * slot_size) } as *mut usize;

        // Zero the lengths array
        for i in 0..capacity {
            unsafe { lengths_ptr.add(i).write(0) };
        }

        Self {
            buf_ptr,
            slot_size,
            capacity,
            lengths_ptr,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Check if the ring has data available for reading.
    pub fn has_data(&self) -> bool {
        self.head.load(Ordering::Acquire) != self.tail.load(Ordering::Relaxed)
    }

    /// Get a mutable slice to write into the next slot.
    ///
    /// Returns `None` if the ring is full (head has caught up to tail).
    /// The writer fills the slice, then calls [`commit_push`] to make it
    /// available to the reader.
    #[allow(dead_code)] // Used by shim direct-write (Phase 73.7)
    #[allow(clippy::mut_from_ref)] // Interior mutability: raw pointer to separate write slot
    pub fn try_push(&self) -> Option<&mut [u8]> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let next_head = (head + 1) % self.capacity;

        if next_head == tail {
            return None; // ring full
        }

        let offset = head * self.slot_size;
        Some(unsafe { core::slice::from_raw_parts_mut(self.buf_ptr.add(offset), self.slot_size) })
    }

    /// Commit a push: record the data length and advance the head.
    #[allow(dead_code)] // Used by shim direct-write (Phase 73.7)
    pub fn commit_push(&self, len: usize) {
        let head = self.head.load(Ordering::Relaxed);
        unsafe {
            self.lengths_ptr.add(head).write(len);
        }
        let next_head = (head + 1) % self.capacity;
        self.head.store(next_head, Ordering::Release);
    }

    /// Get a slice to read from the next available slot.
    ///
    /// Returns `None` if the ring is empty. After processing, call
    /// [`commit_pop`] to advance the tail.
    pub fn try_pop(&self) -> Option<(&[u8], usize)> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);

        if head == tail {
            return None; // ring empty
        }

        let offset = tail * self.slot_size;
        let len = unsafe { self.lengths_ptr.add(tail).read() };
        let data = unsafe { core::slice::from_raw_parts(self.buf_ptr.add(offset), len) };
        Some((data, len))
    }

    /// Advance the tail after a successful `try_pop`.
    pub fn commit_pop(&self) {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % self.capacity;
        self.tail.store(next_tail, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Max region: depth=4, slot_size=64 → capacity=5
    // 5 * 64 + 5 * 8 = 360 bytes (on 64-bit). Use 512 to be safe.
    const TEST_REGION_SIZE: usize = 512;

    #[test]
    fn empty_ring() {
        let mut region = [0u8; TEST_REGION_SIZE];
        let ring = unsafe { SpscRing::init(region.as_mut_ptr(), 64, 4) };
        assert!(!ring.has_data());
        assert!(ring.try_pop().is_none());
    }

    #[test]
    fn push_pop_single() {
        let mut region = [0u8; TEST_REGION_SIZE];
        let ring = unsafe { SpscRing::init(region.as_mut_ptr(), 64, 4) };

        let slot = ring.try_push().unwrap();
        slot[..3].copy_from_slice(b"abc");
        ring.commit_push(3);

        assert!(ring.has_data());
        let (data, len) = ring.try_pop().unwrap();
        assert_eq!(len, 3);
        assert_eq!(&data[..3], b"abc");
        ring.commit_pop();

        assert!(!ring.has_data());
    }

    #[test]
    fn fifo_ordering() {
        let mut region = [0u8; TEST_REGION_SIZE];
        let ring = unsafe { SpscRing::init(region.as_mut_ptr(), 64, 4) };

        // Push 4 messages
        for i in 0u8..4 {
            let slot = ring.try_push().unwrap();
            slot[0] = i;
            ring.commit_push(1);
        }

        // Pop in FIFO order
        for i in 0u8..4 {
            let (data, _) = ring.try_pop().unwrap();
            assert_eq!(data[0], i);
            ring.commit_pop();
        }
    }

    #[test]
    fn full_ring_rejects_push() {
        let mut region = [0u8; TEST_REGION_SIZE];
        let ring = unsafe { SpscRing::init(region.as_mut_ptr(), 64, 2) };

        // depth=2 → capacity=3 → can hold 2 items
        let s1 = ring.try_push().unwrap();
        s1[0] = 1;
        ring.commit_push(1);

        let s2 = ring.try_push().unwrap();
        s2[0] = 2;
        ring.commit_push(1);

        // Ring full
        assert!(ring.try_push().is_none());

        // Pop one → can push again
        ring.try_pop();
        ring.commit_pop();
        assert!(ring.try_push().is_some());
    }

    #[test]
    fn wrap_around() {
        let mut region = [0u8; TEST_REGION_SIZE];
        let ring = unsafe { SpscRing::init(region.as_mut_ptr(), 64, 2) };

        // Fill and drain multiple times to exercise wrap-around
        for round in 0u8..5 {
            for j in 0u8..2 {
                let slot = ring.try_push().unwrap();
                slot[0] = round * 10 + j;
                ring.commit_push(1);
            }

            for j in 0u8..2 {
                let (data, _) = ring.try_pop().unwrap();
                assert_eq!(data[0], round * 10 + j);
                ring.commit_pop();
            }
        }
    }

    #[test]
    fn region_size_calculation() {
        // depth=4, slot_size=64 → capacity=5
        // 5 * 64 (data) + 5 * 8 (lengths on 64-bit) = 360
        assert_eq!(
            SpscRing::region_size(4, 64),
            5 * 64 + 5 * core::mem::size_of::<usize>()
        );
    }
}
