//! Free-list allocator for zenoh-pico memory management
//!
//! Provides `z_malloc`, `z_realloc`, `z_free` implementations.
//! Uses a simple first-fit free-list allocator backed by a static heap.
//!
//! Heap size: 64KB default, 128KB when `link-tls` is enabled
//! (mbedTLS needs ~40KB for TLS context, certificates, and crypto).

use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[cfg(feature = "link-tls")]
const HEAP_SIZE: usize = 128 * 1024;
#[cfg(not(feature = "link-tls"))]
const HEAP_SIZE: usize = 64 * 1024;

/// Static heap memory.
///
/// Accessed only via raw pointers to avoid `static mut` references.
/// Safety: single-threaded bare-metal, no concurrent access.
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

/// Block header stored before each allocation.
#[repr(C)]
struct BlockHeader {
    /// Size of the usable region (excluding this header).
    size: usize,
    /// Pointer to the next free block (null if allocated or last free block).
    next_free: *mut BlockHeader,
}

const HEADER_SIZE: usize = core::mem::size_of::<BlockHeader>();
const ALIGN: usize = 8;

/// Head of the free list (stored as raw pointer via AtomicUsize).
static FREE_LIST_PTR: AtomicUsize = AtomicUsize::new(0);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

fn get_free_list() -> *mut BlockHeader {
    FREE_LIST_PTR.load(Ordering::Relaxed) as *mut BlockHeader
}

fn set_free_list(ptr: *mut BlockHeader) {
    FREE_LIST_PTR.store(ptr as usize, Ordering::Relaxed);
}

/// Initialize the free list with a single block spanning the entire heap.
unsafe fn init_heap() {
    let heap_ptr = ptr::addr_of_mut!(HEAP_MEM) as *mut u8 as *mut BlockHeader;
    unsafe {
        (*heap_ptr).size = HEAP_SIZE - HEADER_SIZE;
        (*heap_ptr).next_free = ptr::null_mut();
    }
    set_free_list(heap_ptr);
    INITIALIZED.store(true, Ordering::Relaxed);
}

/// Align a value up to the given alignment.
#[inline]
const fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

/// Allocate memory from the free-list allocator (8-byte aligned).
///
/// Uses first-fit strategy. Splits blocks if the remainder is large enough
/// to hold another header + minimum allocation.
#[unsafe(no_mangle)]
pub extern "C" fn z_malloc(size: usize) -> *mut core::ffi::c_void {
    if size == 0 {
        return ptr::null_mut();
    }

    let aligned_size = align_up(size, ALIGN);

    unsafe {
        if !INITIALIZED.load(Ordering::Relaxed) {
            init_heap();
        }

        // First-fit search through the free list
        let mut prev: *mut BlockHeader = ptr::null_mut();
        let mut current = get_free_list();

        while !current.is_null() {
            if (*current).size >= aligned_size {
                // Found a fitting block
                let remainder = (*current).size - aligned_size;

                if remainder > HEADER_SIZE + ALIGN {
                    // Split: create a new free block after the allocated region
                    let new_block = (current as *mut u8).add(HEADER_SIZE + aligned_size)
                        as *mut BlockHeader;
                    (*new_block).size = remainder - HEADER_SIZE;
                    (*new_block).next_free = (*current).next_free;

                    (*current).size = aligned_size;

                    // Replace current in the free list with new_block
                    if prev.is_null() {
                        set_free_list(new_block);
                    } else {
                        (*prev).next_free = new_block;
                    }
                } else {
                    // Use the whole block (don't split — remainder too small)
                    if prev.is_null() {
                        set_free_list((*current).next_free);
                    } else {
                        (*prev).next_free = (*current).next_free;
                    }
                }

                // Mark as allocated (next_free = null as sentinel)
                (*current).next_free = ptr::null_mut();

                // Return pointer past the header
                return (current as *mut u8).add(HEADER_SIZE) as *mut core::ffi::c_void;
            }

            prev = current;
            current = (*current).next_free;
        }

        // No fitting block found
        ptr::null_mut()
    }
}

/// Reallocate memory.
///
/// Allocates a new block, copies old data, and frees the old block.
#[unsafe(no_mangle)]
pub extern "C" fn z_realloc(
    old_ptr: *mut core::ffi::c_void,
    size: usize,
) -> *mut core::ffi::c_void {
    if old_ptr.is_null() {
        return z_malloc(size);
    }
    if size == 0 {
        z_free(old_ptr);
        return ptr::null_mut();
    }

    let new_ptr = z_malloc(size);
    if new_ptr.is_null() {
        return ptr::null_mut();
    }

    // Copy old data (up to the smaller of old and new sizes)
    unsafe {
        let old_header = (old_ptr as *mut u8).sub(HEADER_SIZE) as *mut BlockHeader;
        let old_size = (*old_header).size;
        let copy_size = if old_size < size { old_size } else { size };
        ptr::copy_nonoverlapping(old_ptr as *const u8, new_ptr as *mut u8, copy_size);
    }

    z_free(old_ptr);
    new_ptr
}

/// Free a previously allocated block.
///
/// Inserts the block back into the free list in address order and
/// coalesces adjacent free blocks.
#[unsafe(no_mangle)]
pub extern "C" fn z_free(ptr: *mut core::ffi::c_void) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        let block = (ptr as *mut u8).sub(HEADER_SIZE) as *mut BlockHeader;

        // Insert into free list in address order (for coalescing)
        let mut prev: *mut BlockHeader = ptr::null_mut();
        let mut current = get_free_list();

        while !current.is_null() && (current as usize) < (block as usize) {
            prev = current;
            current = (*current).next_free;
        }

        // Insert block between prev and current
        (*block).next_free = current;
        if prev.is_null() {
            set_free_list(block);
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
