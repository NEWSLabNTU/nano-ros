//! Lock-free triple buffer for `KEEP_LAST(1)` subscriptions.
//!
//! Three equally-sized byte slots rotate through write → middle → read roles
//! via a single `AtomicU8`. The writer (network callback) always has a buffer
//! to write into and never blocks. The reader (executor dispatch) always gets
//! the latest complete message.
//!
//! The triple buffer does not own its memory — it operates on a region
//! provided by the arena.

use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// Packed state: `write_idx | (middle_idx << 2) | (read_idx << 4) | (dirty << 6)`.
///
/// Each index is 2 bits (0–2). `dirty` indicates the middle buffer has newer
/// data than the read buffer.
const DIRTY_BIT: u8 = 1 << 6;

#[inline]
const fn pack_state(write: u8, middle: u8, read: u8, dirty: bool) -> u8 {
    write | (middle << 2) | (read << 4) | (if dirty { DIRTY_BIT } else { 0 })
}

#[inline]
const fn unpack_write(state: u8) -> u8 {
    state & 0x03
}

#[inline]
const fn unpack_middle(state: u8) -> u8 {
    (state >> 2) & 0x03
}

#[inline]
const fn unpack_read(state: u8) -> u8 {
    (state >> 4) & 0x03
}

#[inline]
const fn is_dirty(state: u8) -> bool {
    state & DIRTY_BIT != 0
}

/// Lock-free triple buffer over a pre-allocated memory region.
///
/// # Layout
///
/// The caller provides a contiguous region of `3 * slot_size` bytes.
/// Slot 0 starts at `buf_ptr`, slot 1 at `buf_ptr + slot_size`, etc.
///
/// # Thread safety
///
/// Safe for single-producer single-consumer (one writer, one reader).
/// On bare-metal single-threaded systems, the atomics degenerate to plain
/// loads/stores (compiler fence only).
pub(crate) struct TripleBuffer {
    buf_ptr: *mut u8,
    slot_size: usize,
    /// Packed indices + dirty flag.
    state: AtomicU8,
    /// Data length written to each slot.
    lengths: [AtomicUsize; 3],
}

// Safety: SPSC usage — one writer (network callback), one reader (executor).
unsafe impl Send for TripleBuffer {}
unsafe impl Sync for TripleBuffer {}

impl TripleBuffer {
    /// Number of buffer slots.
    pub const SLOT_COUNT: usize = 3;

    /// Initialize a triple buffer over a pre-allocated region.
    ///
    /// # Safety
    ///
    /// `buf_ptr` must point to a region of at least `3 * slot_size` writable
    /// bytes that remains valid for the lifetime of this `TripleBuffer`.
    pub unsafe fn init(buf_ptr: *mut u8, slot_size: usize) -> Self {
        Self {
            buf_ptr,
            slot_size,
            // write=0, middle=1, read=2, dirty=false
            state: AtomicU8::new(pack_state(0, 1, 2, false)),
            lengths: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
        }
    }

    /// Get a mutable slice for the writer to fill. Never blocks.
    ///
    /// The returned slice points to the current write slot. The writer fills
    /// it with data, then calls [`writer_publish`] to make it available.
    #[allow(dead_code)] // Used by shim direct-write (Phase 73.7)
    #[allow(clippy::mut_from_ref)] // Interior mutability: raw pointer to separate write slot
    pub fn write_slot(&self) -> &mut [u8] {
        let idx = unpack_write(self.state.load(Ordering::Relaxed)) as usize;
        unsafe {
            core::slice::from_raw_parts_mut(self.buf_ptr.add(idx * self.slot_size), self.slot_size)
        }
    }

    /// Writer is done — swap write and middle slots, mark dirty.
    ///
    /// After this call, the data written to the write slot becomes the latest
    /// available message. The previous middle slot becomes the new write slot.
    #[allow(dead_code)] // Used by shim direct-write (Phase 73.7)
    pub fn writer_publish(&self, len: usize) {
        let old = self.state.load(Ordering::Relaxed);
        let w = unpack_write(old);
        let m = unpack_middle(old);
        let r = unpack_read(old);

        // Store length for the slot we just wrote
        self.lengths[w as usize].store(len, Ordering::Relaxed);

        // Swap write ↔ middle, set dirty
        self.state
            .store(pack_state(m, w, r, true), Ordering::Release);
    }

    /// Swap middle and read if new data is available.
    ///
    /// Returns the read slot data and its length, or `None` if no new data
    /// since the last `reader_acquire`.
    pub fn reader_acquire(&self) -> Option<(&[u8], usize)> {
        let old = self.state.load(Ordering::Acquire);
        if !is_dirty(old) {
            return None;
        }

        let w = unpack_write(old);
        let m = unpack_middle(old);
        let r = unpack_read(old);

        // Swap middle ↔ read, clear dirty
        self.state
            .store(pack_state(w, r, m, false), Ordering::Relaxed);

        let idx = m as usize; // the old middle is now our read slot
        let len = self.lengths[idx].load(Ordering::Relaxed);
        let data =
            unsafe { core::slice::from_raw_parts(self.buf_ptr.add(idx * self.slot_size), len) };
        Some((data, len))
    }

    /// Check if new data is available without consuming it.
    pub fn has_data(&self) -> bool {
        is_dirty(self.state.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_write_read() {
        let mut region = [0u8; 3 * 64];
        let tb = unsafe { TripleBuffer::init(region.as_mut_ptr(), 64) };

        // No data initially
        assert!(!tb.has_data());
        assert!(tb.reader_acquire().is_none());

        // Write
        let slot = tb.write_slot();
        slot[..5].copy_from_slice(b"hello");
        tb.writer_publish(5);

        // Read
        assert!(tb.has_data());
        let (data, len) = tb.reader_acquire().unwrap();
        assert_eq!(len, 5);
        assert_eq!(&data[..5], b"hello");

        // No more data until next write
        assert!(!tb.has_data());
        assert!(tb.reader_acquire().is_none());
    }

    #[test]
    fn latest_value_semantics() {
        let mut region = [0u8; 3 * 64];
        let tb = unsafe { TripleBuffer::init(region.as_mut_ptr(), 64) };

        // Write three messages without reading
        for i in 0u8..3 {
            let slot = tb.write_slot();
            slot[0] = i;
            tb.writer_publish(1);
        }

        // Reader sees only the latest (2)
        let (data, len) = tb.reader_acquire().unwrap();
        assert_eq!(len, 1);
        assert_eq!(data[0], 2);

        // No more data
        assert!(tb.reader_acquire().is_none());
    }

    #[test]
    fn writer_never_blocks() {
        let mut region = [0u8; 3 * 64];
        let tb = unsafe { TripleBuffer::init(region.as_mut_ptr(), 64) };

        // Write 100 times without reading — should never block or panic
        for i in 0u8..100 {
            let slot = tb.write_slot();
            slot[0] = i;
            tb.writer_publish(1);
        }

        // Reader gets the latest
        let (data, _) = tb.reader_acquire().unwrap();
        assert_eq!(data[0], 99);
    }

    #[test]
    fn interleaved_write_read() {
        let mut region = [0u8; 3 * 64];
        let tb = unsafe { TripleBuffer::init(region.as_mut_ptr(), 64) };

        for i in 0u8..10 {
            let slot = tb.write_slot();
            slot[0] = i;
            tb.writer_publish(1);

            let (data, _) = tb.reader_acquire().unwrap();
            assert_eq!(data[0], i);
        }
    }
}
