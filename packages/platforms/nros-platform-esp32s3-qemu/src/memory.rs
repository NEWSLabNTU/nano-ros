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

// Phase 117.1 — 256 KiB DDS heap in internal SRAM as the
// transitional default. The end-state design routes the heap to
// PSRAM via `#[link_section = ".ext_ram.bss"]` + esp-hal's
// `psram::init` (1 MiB+ budget); that wiring is gated on the
// board crate's PSRAM init landing as Phase 117.2b. Until then
// the 256 KiB internal-SRAM carve-out fits within ESP32-S3's
// 512 KiB DRAM (vs the C3 path's 192 KiB cap for its 400 KiB
// chip) and is enough for dust-dds's `DcpsDomainParticipant`
// builtin entities to compile and link, while letting users
// exercise the rest of the bring-up before PSRAM lands.
// Conservative 192 KiB matches the ESP32-C3 path's tested cap.
// PSRAM-routed 1 MiB heap is gated on Phase 117.2b's psram::init
// wiring; until that lands, internal-SRAM-only carve-out must
// leave headroom for `.bss` / `.text` / `.rodata` / esp-hal runtime
// fixtures on top.
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
