//! Free-list heap allocator for bare-metal STM32F4 (64 KB).

use zpico_alloc::FreeListHeap;

static HEAP: FreeListHeap<{ 64 * 1024 }> = FreeListHeap::new();

pub fn alloc(size: usize) -> *mut core::ffi::c_void { HEAP.alloc(size) }
pub fn realloc(ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void { HEAP.realloc(ptr, size) }
pub fn dealloc(ptr: *mut core::ffi::c_void) { HEAP.free(ptr) }
