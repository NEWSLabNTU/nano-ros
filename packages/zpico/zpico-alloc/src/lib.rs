//! Free-list allocator for zenoh-pico `z_malloc`/`z_free` on bare-metal.
//!
//! Provides a generic [`FreeListHeap`] that platform crates instantiate as a
//! static with their desired heap size. The allocator uses first-fit search
//! with address-ordered free list and two-pass coalescing on free.
//!
//! ## Slab fast-path
//!
//! Allocations ≤ `SLAB_SLOT_SIZE` (64) bytes are served from a small slab cache
//! (8 slots × 64 bytes = 512 bytes) with O(1) bitmap-based alloc/free. This
//! targets zenoh-pico's per-message string field allocations (short-lived
//! `z_malloc` + `z_free` pairs during CDR parsing). Larger allocations fall
//! through to the free-list.
//!
//! # Usage
//!
//! ```rust,ignore
//! use zpico_alloc::FreeListHeap;
//!
//! static HEAP: FreeListHeap<{64 * 1024}> = FreeListHeap::new();
//!
//! #[unsafe(no_mangle)]
//! pub extern "C" fn z_malloc(size: usize) -> *mut core::ffi::c_void {
//!     HEAP.alloc(size)
//! }
//! #[unsafe(no_mangle)]
//! pub extern "C" fn z_free(ptr: *mut core::ffi::c_void) {
//!     HEAP.free(ptr)
//! }
//! #[unsafe(no_mangle)]
//! pub extern "C" fn z_realloc(ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void {
//!     HEAP.realloc(ptr, size)
//! }
//! ```

#![no_std]

use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

const ALIGN: usize = 8;
const HEADER_SIZE: usize = core::mem::size_of::<BlockHeader>();

/// Slab slot size in bytes. Allocations ≤ this size use the O(1) slab cache.
///
/// 64 bytes covers zenoh-pico's common string field allocations (topic names,
/// key expressions, type hashes) which are typically 20–50 bytes.
const SLAB_SLOT_SIZE: usize = 64;

/// Number of slab slots. 8 slots = 512 bytes total slab region.
///
/// zenoh-pico parses at most a few string fields per message, so 8 slots
/// provides ample headroom for concurrent short-lived allocations.
const SLAB_SLOT_COUNT: usize = 8;

/// Total slab region size in bytes.
const SLAB_REGION_SIZE: usize = SLAB_SLOT_SIZE * SLAB_SLOT_COUNT;

/// Block header stored immediately before each allocation.
#[repr(C)]
struct BlockHeader {
    /// Usable region size (excludes this header).
    size: usize,
    /// Next free block (null when allocated or last in free list).
    next_free: *mut BlockHeader,
}

/// Align `val` up to 8-byte boundary.
#[inline]
const fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

/// First-fit free-list allocator backed by a static `[u8; N]` heap,
/// with an O(1) slab fast-path for small allocations.
///
/// Single-threaded bare-metal only — uses `Relaxed` atomics for the free-list
/// head pointer and initialization flag. Not safe for multi-threaded use.
pub struct FreeListHeap<const N: usize> {
    heap: UnsafeCell<[u8; N]>,
    free_list: AtomicUsize,
    initialized: AtomicBool,
    /// Slab region: 8 slots × 64 bytes, separate from the main heap.
    slab: UnsafeCell<[u8; SLAB_REGION_SIZE]>,
    /// Bitmap of free slab slots (bit set = free). Starts as 0xFF (all free).
    slab_free_bitmap: AtomicU8,
    #[cfg(feature = "stats")]
    used_bytes: AtomicUsize,
    #[cfg(feature = "stats")]
    peak_bytes: AtomicUsize,
}

// Safety: bare-metal single-threaded. The AtomicUsize/AtomicBool provide
// interior mutability without `static mut`.
unsafe impl<const N: usize> Sync for FreeListHeap<N> {}

impl<const N: usize> Default for FreeListHeap<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> FreeListHeap<N> {
    /// Create a new heap. Use in a `static`:
    /// ```rust,ignore
    /// static HEAP: FreeListHeap<{64 * 1024}> = FreeListHeap::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            heap: UnsafeCell::new([0u8; N]),
            free_list: AtomicUsize::new(0),
            initialized: AtomicBool::new(false),
            slab: UnsafeCell::new([0u8; SLAB_REGION_SIZE]),
            slab_free_bitmap: AtomicU8::new(0xFF), // all 8 slots free
            #[cfg(feature = "stats")]
            used_bytes: AtomicUsize::new(0),
            #[cfg(feature = "stats")]
            peak_bytes: AtomicUsize::new(0),
        }
    }

    /// Lazily initialize the free list with one block spanning the whole heap.
    #[inline]
    unsafe fn ensure_init(&self) {
        if !self.initialized.load(Ordering::Relaxed) {
            let heap_ptr = self.heap.get() as *mut u8 as *mut BlockHeader;
            unsafe {
                (*heap_ptr).size = N - HEADER_SIZE;
                (*heap_ptr).next_free = ptr::null_mut();
            }
            self.free_list.store(heap_ptr as usize, Ordering::Relaxed);
            self.initialized.store(true, Ordering::Relaxed);
        }
    }

    #[inline]
    fn get_free_list(&self) -> *mut BlockHeader {
        self.free_list.load(Ordering::Relaxed) as *mut BlockHeader
    }

    #[inline]
    fn set_free_list(&self, ptr: *mut BlockHeader) {
        self.free_list.store(ptr as usize, Ordering::Relaxed);
    }

    // ── Slab fast-path ─────────────────────────────────────────────────

    /// Base pointer of the slab region.
    #[inline]
    fn slab_base(&self) -> *mut u8 {
        self.slab.get() as *mut u8
    }

    /// Check if `ptr` points into the slab region.
    #[inline]
    fn is_in_slab(&self, ptr: *mut u8) -> bool {
        let base = self.slab_base() as usize;
        let addr = ptr as usize;
        addr >= base && addr < base + SLAB_REGION_SIZE
    }

    /// Try to allocate from the slab. Returns null if no free slot or size too large.
    #[inline]
    fn slab_alloc(&self, size: usize) -> *mut core::ffi::c_void {
        if size > SLAB_SLOT_SIZE {
            return ptr::null_mut();
        }

        let bitmap = self.slab_free_bitmap.load(Ordering::Relaxed);
        if bitmap == 0 {
            return ptr::null_mut(); // all slots occupied
        }

        // Find first set bit (first free slot) — O(1)
        let slot = bitmap.trailing_zeros() as usize;

        // Clear the bit (mark as occupied)
        self.slab_free_bitmap
            .store(bitmap & !(1 << slot), Ordering::Relaxed);

        #[cfg(feature = "stats")]
        {
            let used =
                self.used_bytes.fetch_add(SLAB_SLOT_SIZE, Ordering::Relaxed) + SLAB_SLOT_SIZE;
            let _ = self.peak_bytes.fetch_max(used, Ordering::Relaxed);
        }

        unsafe { self.slab_base().add(slot * SLAB_SLOT_SIZE) as *mut core::ffi::c_void }
    }

    /// Return a slab slot. Caller must verify `is_in_slab(ptr)` first.
    #[inline]
    fn slab_free(&self, ptr: *mut u8) {
        let offset = ptr as usize - self.slab_base() as usize;
        let slot = offset / SLAB_SLOT_SIZE;

        // Set the bit (mark as free)
        let bitmap = self.slab_free_bitmap.load(Ordering::Relaxed);
        self.slab_free_bitmap
            .store(bitmap | (1 << slot), Ordering::Relaxed);

        #[cfg(feature = "stats")]
        self.used_bytes.fetch_sub(SLAB_SLOT_SIZE, Ordering::Relaxed);
    }

    // ── Public API ─────────────────────────────────────────────────────

    /// Allocate `size` bytes (8-byte aligned). Returns null on failure.
    ///
    /// Allocations ≤ 64 bytes try the slab cache first (O(1)). Larger
    /// allocations use first-fit free-list search. Splits blocks when the
    /// remainder is large enough for another header + minimum allocation.
    pub fn alloc(&self, size: usize) -> *mut core::ffi::c_void {
        if size == 0 {
            return ptr::null_mut();
        }

        // Slab fast-path for small allocations
        if size <= SLAB_SLOT_SIZE {
            let ptr = self.slab_alloc(size);
            if !ptr.is_null() {
                return ptr;
            }
            // Slab full — fall through to free-list
        }

        let aligned_size = align_up(size, ALIGN);

        unsafe {
            self.ensure_init();

            let mut prev: *mut BlockHeader = ptr::null_mut();
            let mut current = self.get_free_list();

            while !current.is_null() {
                if (*current).size >= aligned_size {
                    let remainder = (*current).size - aligned_size;

                    if remainder > HEADER_SIZE + ALIGN {
                        // Split: new free block after the allocated region
                        let new_block = (current as *mut u8).add(HEADER_SIZE + aligned_size)
                            as *mut BlockHeader;
                        (*new_block).size = remainder - HEADER_SIZE;
                        (*new_block).next_free = (*current).next_free;
                        (*current).size = aligned_size;

                        if prev.is_null() {
                            self.set_free_list(new_block);
                        } else {
                            (*prev).next_free = new_block;
                        }
                    } else {
                        // Use whole block (remainder too small to split)
                        if prev.is_null() {
                            self.set_free_list((*current).next_free);
                        } else {
                            (*prev).next_free = (*current).next_free;
                        }
                    }

                    (*current).next_free = ptr::null_mut();

                    #[cfg(feature = "stats")]
                    {
                        let used = self
                            .used_bytes
                            .fetch_add((*current).size + HEADER_SIZE, Ordering::Relaxed)
                            + (*current).size
                            + HEADER_SIZE;
                        let _ = self.peak_bytes.fetch_max(used, Ordering::Relaxed);
                    }

                    return (current as *mut u8).add(HEADER_SIZE) as *mut core::ffi::c_void;
                }

                prev = current;
                current = (*current).next_free;
            }

            ptr::null_mut()
        }
    }

    /// Reallocate: alloc new block, copy old data, free old block.
    pub fn realloc(&self, old_ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void {
        if old_ptr.is_null() {
            return self.alloc(size);
        }
        if size == 0 {
            self.free(old_ptr);
            return ptr::null_mut();
        }

        let new_ptr = self.alloc(size);
        if new_ptr.is_null() {
            return ptr::null_mut();
        }

        unsafe {
            // Determine old size: slab slots are fixed SLAB_SLOT_SIZE,
            // free-list blocks store size in the header.
            let old_size = if self.is_in_slab(old_ptr as *mut u8) {
                SLAB_SLOT_SIZE
            } else {
                let old_header = (old_ptr as *mut u8).sub(HEADER_SIZE) as *mut BlockHeader;
                (*old_header).size
            };
            let copy_size = if old_size < size { old_size } else { size };
            ptr::copy_nonoverlapping(old_ptr as *const u8, new_ptr as *mut u8, copy_size);
        }

        self.free(old_ptr);
        new_ptr
    }

    /// Return a block to the allocator.
    ///
    /// Slab pointers are returned to the slab bitmap (O(1)). Free-list
    /// pointers are inserted in address order with coalescing.
    pub fn free(&self, ptr: *mut core::ffi::c_void) {
        if ptr.is_null() {
            return;
        }

        // Slab fast-path
        if self.is_in_slab(ptr as *mut u8) {
            self.slab_free(ptr as *mut u8);
            return;
        }

        unsafe {
            let block = (ptr as *mut u8).sub(HEADER_SIZE) as *mut BlockHeader;

            #[cfg(feature = "stats")]
            self.used_bytes
                .fetch_sub((*block).size + HEADER_SIZE, Ordering::Relaxed);

            // Insert into free list in address order
            let mut prev: *mut BlockHeader = ptr::null_mut();
            let mut current = self.get_free_list();

            while !current.is_null() && (current as usize) < (block as usize) {
                prev = current;
                current = (*current).next_free;
            }

            (*block).next_free = current;
            if prev.is_null() {
                self.set_free_list(block);
            } else {
                (*prev).next_free = block;
            }

            // Coalesce with next block if adjacent
            if !current.is_null() {
                let block_end = (block as *mut u8).add(HEADER_SIZE + (*block).size);
                if block_end == current as *mut u8 {
                    (*block).size += HEADER_SIZE + (*current).size;
                    (*block).next_free = (*current).next_free;
                }
            }

            // Coalesce with previous block if adjacent
            if !prev.is_null() {
                let prev_end = (prev as *mut u8).add(HEADER_SIZE + (*prev).size);
                if prev_end == block as *mut u8 {
                    (*prev).size += HEADER_SIZE + (*block).size;
                    (*prev).next_free = (*block).next_free;
                }
            }
        }
    }

    /// Current heap usage in bytes (slab + free-list, including headers).
    ///
    /// Only available with the `stats` feature.
    #[cfg(feature = "stats")]
    pub fn used(&self) -> usize {
        self.used_bytes.load(Ordering::Relaxed)
    }

    /// Peak heap usage in bytes since boot.
    ///
    /// Only available with the `stats` feature.
    #[cfg(feature = "stats")]
    pub fn peak(&self) -> usize {
        self.peak_bytes.load(Ordering::Relaxed)
    }

    /// Free bytes remaining (approximate — does not account for fragmentation).
    ///
    /// Only available with the `stats` feature.
    #[cfg(feature = "stats")]
    pub fn free_bytes(&self) -> usize {
        (N + SLAB_REGION_SIZE).saturating_sub(self.used_bytes.load(Ordering::Relaxed))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slab_alloc_and_free() {
        let heap: FreeListHeap<1024> = FreeListHeap::new();

        // Allocate a small buffer — should hit slab
        let p1 = heap.alloc(32);
        assert!(!p1.is_null());
        assert!(heap.is_in_slab(p1 as *mut u8));

        // Free and reallocate — should reuse same slot
        heap.free(p1);
        let p2 = heap.alloc(32);
        assert!(!p2.is_null());
        assert!(heap.is_in_slab(p2 as *mut u8));
        assert_eq!(p1, p2); // same slot reused (first free bit)
        heap.free(p2);
    }

    #[test]
    fn slab_exhaustion_falls_through() {
        let heap: FreeListHeap<4096> = FreeListHeap::new();

        // Fill all 8 slab slots
        let mut ptrs = [ptr::null_mut(); SLAB_SLOT_COUNT];
        for p in &mut ptrs {
            *p = heap.alloc(16);
            assert!(!p.is_null());
            assert!(heap.is_in_slab(*p as *mut u8));
        }

        // 9th small alloc falls through to free-list
        let overflow = heap.alloc(16);
        assert!(!overflow.is_null());
        assert!(!heap.is_in_slab(overflow as *mut u8));

        // Free all
        for p in &ptrs {
            heap.free(*p);
        }
        heap.free(overflow);
    }

    #[test]
    fn large_alloc_skips_slab() {
        let heap: FreeListHeap<4096> = FreeListHeap::new();

        // > SLAB_SLOT_SIZE goes directly to free-list
        let p = heap.alloc(128);
        assert!(!p.is_null());
        assert!(!heap.is_in_slab(p as *mut u8));
        heap.free(p);
    }

    #[test]
    fn realloc_slab_to_freelist() {
        let heap: FreeListHeap<4096> = FreeListHeap::new();

        // Small alloc → slab
        let p1 = heap.alloc(32);
        assert!(heap.is_in_slab(p1 as *mut u8));

        // Write data
        unsafe {
            ptr::write_bytes(p1 as *mut u8, 0xAB, 32);
        }

        // Realloc to larger → moves to free-list, copies data
        let p2 = heap.realloc(p1, 128);
        assert!(!p2.is_null());
        assert!(!heap.is_in_slab(p2 as *mut u8));

        // Verify data was copied
        unsafe {
            let slice = core::slice::from_raw_parts(p2 as *const u8, 32);
            assert!(slice.iter().all(|&b| b == 0xAB));
        }

        heap.free(p2);
    }

    #[test]
    fn zero_size_returns_null() {
        let heap: FreeListHeap<1024> = FreeListHeap::new();
        assert!(heap.alloc(0).is_null());
    }

    #[test]
    fn free_null_is_noop() {
        let heap: FreeListHeap<1024> = FreeListHeap::new();
        heap.free(ptr::null_mut()); // should not panic
    }

    #[test]
    fn freelist_coalescing() {
        let heap: FreeListHeap<4096> = FreeListHeap::new();

        // Allocate three adjacent blocks (> SLAB_SLOT_SIZE to skip slab)
        let p1 = heap.alloc(128);
        let p2 = heap.alloc(128);
        let p3 = heap.alloc(128);
        assert!(!p1.is_null());
        assert!(!p2.is_null());
        assert!(!p3.is_null());

        // Free middle, then neighbours — should coalesce
        heap.free(p2);
        heap.free(p1);
        heap.free(p3);

        // Should be able to allocate a large block from coalesced region
        let big = heap.alloc(384);
        assert!(!big.is_null());
        heap.free(big);
    }

    #[cfg(feature = "stats")]
    #[test]
    fn stats_track_slab_and_freelist() {
        let heap: FreeListHeap<4096> = FreeListHeap::new();

        assert_eq!(heap.used(), 0);

        // Slab alloc
        let p1 = heap.alloc(32);
        assert_eq!(heap.used(), SLAB_SLOT_SIZE); // slab charges full slot

        // Free-list alloc
        let p2 = heap.alloc(128);
        let used_after_both = heap.used();
        assert!(used_after_both > SLAB_SLOT_SIZE);

        // Free both
        heap.free(p1);
        heap.free(p2);
        assert_eq!(heap.used(), 0);

        // Peak should reflect the maximum
        assert!(heap.peak() >= used_after_both);
    }
}
