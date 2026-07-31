//! Free-list heap allocator for bare-metal STM32F4 (32 KB).
//!
//! Sized at 32 KB to leave headroom on the STM32F429ZI's 192 KB SRAM
//! for the Executor arena (up to ~18 KB on the action variants), the
//! smoltcp socket pool, and the rest of `.bss`/`.uninit`. zenoh-pico's
//! TCP session uses ~16 KB peak, so 32 KB leaves ~2× margin for
//! fragmentation.

use zpico_alloc::FreeListHeap;

/// Phase 204.5 — `NROS_HEAP_SIZE` (compile-time env, decimal bytes, set in
/// the example's `.cargo/config.toml` `[env]`) overrides the 32 KB default.
const DEFAULT_HEAP_SIZE: usize = 32 * 1024;
const HEAP_SIZE: usize = match option_env!("NROS_HEAP_SIZE") {
    Some(s) => parse_usize(s),
    None => DEFAULT_HEAP_SIZE,
};

const fn parse_usize(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut n: usize = 0;
    while i < bytes.len() {
        let b = bytes[i];
        assert!(
            b >= b'0' && b <= b'9',
            "NROS_HEAP_SIZE must be decimal bytes"
        );
        n = n * 10 + (b - b'0') as usize;
        i += 1;
    }
    assert!(n > 0, "NROS_HEAP_SIZE must be > 0");
    n
}

#[unsafe(link_section = ".ccmram")]
static HEAP: FreeListHeap<HEAP_SIZE> = FreeListHeap::new();

pub fn alloc(size: usize) -> *mut core::ffi::c_void {
    HEAP.alloc(size)
}
pub fn realloc(ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void {
    HEAP.realloc(ptr, size)
}
pub fn dealloc(ptr: *mut core::ffi::c_void) {
    HEAP.free(ptr)
}

/// Bytes currently allocated from the heap (Phase 230 1b / RFC-0034 —
/// backs `nros_platform_heap_used_bytes`). Tracked by `FreeListHeap`'s
/// `stats` feature.
pub fn used() -> usize {
    HEAP.used()
}

/// Total managed heap capacity in bytes (used + free).
pub fn total() -> usize {
    HEAP.capacity()
}
