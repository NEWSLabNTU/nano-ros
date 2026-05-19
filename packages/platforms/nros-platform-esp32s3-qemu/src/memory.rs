//! Free-list heap allocator for bare-metal ESP32-S3 (32 KiB default,
//! 1 MiB with `dds-heap`).
//!
//! ESP32-S3 has 512 KiB internal SRAM PLUS up to 16 MiB octal PSRAM
//! (8 MiB on the QEMU-modeled part). The `dds-heap` budget therefore
//! has no analogue to the C3 path's 192 KiB cramp — dust-dds's
//! `DcpsDomainParticipant` + builtin actors live in PSRAM, leaving
//! internal SRAM for stack + smoltcp buffers + the esp-hal runtime.
//!
//! The heap region is provided by `zpico-alloc::FreeListHeap` as a
//! `static` carve-out. The board crate (`nros-board-esp32s3-qemu`)
//! is responsible for ensuring the link script pins this static
//! into the PSRAM region (`.ext_ram.bss` / equivalent) — without
//! that the carve-out lands in internal SRAM and overruns by ~512
//! KiB. See the board crate's `memory.x` for the section attribute
//! wire-up.
//!
//! 1 MiB matches dust-dds's typical RTPS participant heap on
//! desktop targets; the cap can be lifted further (`dds-heap-large`
//! feature) if discovery starts to thrash.

use zpico_alloc::FreeListHeap;

// Phase 117.1 — 1 MiB DDS heap (PSRAM-backed). Pre-117 ESP32-C3
// path was capped at 192 KiB to fit within 400 KiB internal SRAM;
// here the heap region is in PSRAM so the budget is no longer
// internal-SRAM-bound.
#[cfg(feature = "dds-heap")]
#[unsafe(link_section = ".ext_ram.bss")]
static HEAP: FreeListHeap<{ 1024 * 1024 }> = FreeListHeap::new();
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
