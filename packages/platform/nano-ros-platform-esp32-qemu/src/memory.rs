//! Bump allocator for zenoh-pico memory management
//!
//! Provides `z_malloc`, `z_realloc`, `z_free` implementations.
//! Uses a simple bump allocator with 32KB heap (no deallocation support).
//!
//! Note: This is separate from `esp_alloc` which provides the global allocator
//! for the `alloc` crate. zenoh-pico calls `z_malloc`/`z_free` directly.

use core::ptr;

const HEAP_SIZE: usize = 32 * 1024;
static mut HEAP_MEM: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
static mut HEAP_POS: usize = 0;

/// Allocate memory from the bump allocator (8-byte aligned)
#[unsafe(no_mangle)]
pub extern "C" fn z_malloc(size: usize) -> *mut core::ffi::c_void {
    unsafe {
        let aligned_pos = (HEAP_POS + 7) & !7;
        let new_pos = aligned_pos + size;

        if new_pos > HEAP_SIZE {
            return ptr::null_mut();
        }

        HEAP_POS = new_pos;
        HEAP_MEM[aligned_pos..].as_mut_ptr() as *mut core::ffi::c_void
    }
}

/// Reallocate memory (bump allocator: allocates new block, no copy)
#[unsafe(no_mangle)]
pub extern "C" fn z_realloc(
    ptr: *mut core::ffi::c_void,
    size: usize,
) -> *mut core::ffi::c_void {
    if ptr.is_null() {
        return z_malloc(size);
    }
    if size == 0 {
        return ptr::null_mut();
    }
    z_malloc(size)
}

/// Free memory (no-op: bump allocator doesn't support deallocation)
#[unsafe(no_mangle)]
pub extern "C" fn z_free(_ptr: *mut core::ffi::c_void) {
    // No-op: bump allocator doesn't support deallocation
}
