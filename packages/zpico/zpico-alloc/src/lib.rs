//! Free-list allocator for zenoh-pico `z_malloc`/`z_free` on bare-metal.
//!
//! Provides a generic [`FreeListHeap`] that platform crates instantiate as a
//! static with their desired heap size. The allocator uses first-fit search
//! with address-ordered free list and two-pass coalescing on free.
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
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const ALIGN: usize = 8;
const HEADER_SIZE: usize = core::mem::size_of::<BlockHeader>();

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

/// First-fit free-list allocator backed by a static `[u8; N]` heap.
///
/// Single-threaded bare-metal only — uses `Relaxed` atomics for the free-list
/// head pointer and initialization flag. Not safe for multi-threaded use.
pub struct FreeListHeap<const N: usize> {
    heap: UnsafeCell<[u8; N]>,
    free_list: AtomicUsize,
    initialized: AtomicBool,
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

    /// Allocate `size` bytes (8-byte aligned). Returns null on failure.
    ///
    /// Uses first-fit search. Splits blocks when the remainder is large
    /// enough for another header + minimum allocation.
    pub fn alloc(&self, size: usize) -> *mut core::ffi::c_void {
        if size == 0 {
            return ptr::null_mut();
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
            let old_header = (old_ptr as *mut u8).sub(HEADER_SIZE) as *mut BlockHeader;
            let old_size = (*old_header).size;
            let copy_size = if old_size < size { old_size } else { size };
            ptr::copy_nonoverlapping(old_ptr as *const u8, new_ptr as *mut u8, copy_size);
        }

        self.free(old_ptr);
        new_ptr
    }

    /// Return a block to the free list with address-ordered coalescing.
    pub fn free(&self, ptr: *mut core::ffi::c_void) {
        if ptr.is_null() {
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

    /// Current heap usage in bytes (header + payload).
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
        N.saturating_sub(self.used_bytes.load(Ordering::Relaxed))
    }
}
