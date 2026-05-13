//! Free-list heap allocator for bare-metal STM32F4 (32 KB).
//!
//! Sized at 32 KB to leave headroom on the STM32F429ZI's 192 KB SRAM
//! for the Executor arena (up to ~18 KB on the action variants), the
//! smoltcp socket pool, and the rest of `.bss`/`.uninit`. zenoh-pico's
//! TCP session uses ~16 KB peak, so 32 KB leaves ~2× margin for
//! fragmentation.

use zpico_alloc::FreeListHeap;

static HEAP: FreeListHeap<{ 32 * 1024 }> = FreeListHeap::new();

pub fn alloc(size: usize) -> *mut core::ffi::c_void {
    HEAP.alloc(size)
}
pub fn realloc(ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void {
    HEAP.realloc(ptr, size)
}
pub fn dealloc(ptr: *mut core::ffi::c_void) {
    HEAP.free(ptr)
}
