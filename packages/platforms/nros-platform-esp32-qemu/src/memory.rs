//! Free-list heap allocator for bare-metal ESP32-C3 (32 KB default,
//! 256 KB with `dds-heap`).
//!
//! ESP32-C3 has 400 KB of SRAM total. The 256 KB DDS budget squeezes
//! dust-dds's `DcpsDomainParticipant` builtin entities into the
//! biggest static carve-out we can spare while leaving headroom for
//! stack, smoltcp buffers, and the esp-hal runtime. Same shape as
//! MPS2-AN385's `dds-heap` feature, sized down for the smaller chip.

use zpico_alloc::FreeListHeap;

// Phase 101.7 — original `dds-heap` carve-out was 256 KB, but that
// pushed `.bss` + `.stack` past the 400 KB DRAM limit on ESP32-C3
// (real-world overflow: ~26 KB). 192 KB leaves enough headroom for
// the smoltcp + esp-hal fixed allocations + stack while still
// fitting dust-dds's `DcpsDomainParticipant` builtin entities.
#[cfg(feature = "dds-heap")]
static HEAP: FreeListHeap<{ 192 * 1024 }> = FreeListHeap::new();
#[cfg(not(feature = "dds-heap"))]
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
