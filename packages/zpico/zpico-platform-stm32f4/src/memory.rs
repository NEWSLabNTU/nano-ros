//! Free-list allocator for zenoh-pico memory management.
//!
//! Delegates to [`zpico_alloc::FreeListHeap`] for `z_malloc`, `z_realloc`,
//! `z_free`.

use zpico_alloc::FreeListHeap;

static HEAP: FreeListHeap<{ 64 * 1024 }> = FreeListHeap::new();

#[unsafe(no_mangle)]
pub extern "C" fn z_malloc(size: usize) -> *mut core::ffi::c_void {
    HEAP.alloc(size)
}

#[unsafe(no_mangle)]
pub extern "C" fn z_realloc(
    ptr: *mut core::ffi::c_void,
    size: usize,
) -> *mut core::ffi::c_void {
    HEAP.realloc(ptr, size)
}

#[unsafe(no_mangle)]
pub extern "C" fn z_free(ptr: *mut core::ffi::c_void) {
    HEAP.free(ptr)
}
