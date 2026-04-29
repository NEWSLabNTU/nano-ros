//! Free-list heap allocator for bare-metal MPS2-AN385.
//!
//! Heap size:
//!   - 64 KB default (zenoh-pico / xrce-dds builds)
//!   - 128 KB with `link-tls` (mbedTLS context + certs + crypto)
//!   - 2 MB with `dds-heap` (Phase 97.3.mps2-an385 — dust-dds
//!     DcpsDomainParticipant builtin entities; same budget as the
//!     FreeRTOS / ThreadX-RV64 slices)

use zpico_alloc::FreeListHeap;

#[cfg(feature = "dds-heap")]
static HEAP: FreeListHeap<{ 2 * 1024 * 1024 }> = FreeListHeap::new();
#[cfg(all(feature = "link-tls", not(feature = "dds-heap")))]
static HEAP: FreeListHeap<{ 128 * 1024 }> = FreeListHeap::new();
#[cfg(not(any(feature = "link-tls", feature = "dds-heap")))]
static HEAP: FreeListHeap<{ 64 * 1024 }> = FreeListHeap::new();

pub fn alloc(size: usize) -> *mut core::ffi::c_void {
    HEAP.alloc(size)
}

pub fn realloc(ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void {
    HEAP.realloc(ptr, size)
}

pub fn dealloc(ptr: *mut core::ffi::c_void) {
    HEAP.free(ptr)
}
