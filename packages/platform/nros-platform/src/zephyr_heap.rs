//! phase-391 W3 — the Zephyr rlsf arena behind `nros_platform_alloc`.
//!
//! On Zephyr the allocation funnel is C
//! (`nros-platform-zephyr/src/platform.c`): `z_malloc` and the compiler's
//! `__rust_alloc` both tail-call `nros_platform_alloc`, which historically was
//! three `k_malloc`/`k_free` lines against `CONFIG_HEAP_MEM_POOL_SIZE`'s
//! `sys_heap`. This module gives that funnel an rlsf-backed arena instead:
//! O(1) alloc/free (the property a safety image needs), and once an image's
//! conf sets `CONFIG_HEAP_MEM_POOL_SIZE=0`, `k_malloc`/`sys_heap_*`
//! garbage-collect out of the link — which is simultaneously the wave's link
//! test that no enabled Zephyr subsystem still needed them.
//!
//! # Concurrency
//!
//! `FreeListHeap` is single-threaded by contract and Zephyr is not: the
//! zenoh-pico read/lease tasks allocate concurrently with the app. The C
//! funnel wraps every call to these exports in a `k_spinlock`
//! (`platform.c`), which is correct because rlsf is O(1) — the critical
//! section is short and bounded. These exports MUST NOT be called from any
//! other path.
//!
//! # Sizing
//!
//! `NROS_ZEPHYR_HEAP_SIZE` (compile-time env, decimal bytes), default 64 KiB —
//! matching the zenoh images' previous `CONFIG_HEAP_MEM_POOL_SIZE=65536`, so a
//! converted image's RAM is net-neutral before any tuning. An image that has
//! NOT set its pool to 0 temporarily carries both arenas; convert the conf in
//! the same change.

use zpico_alloc::FreeListHeap;

const DEFAULT_HEAP_SIZE: usize = 64 * 1024;

const HEAP_SIZE: usize = match option_env!("NROS_ZEPHYR_HEAP_SIZE") {
    Some(s) => parse_usize(s),
    None => DEFAULT_HEAP_SIZE,
};

/// `const`-evaluable decimal parse — same pattern as the bare-metal ports'
/// `memory.rs` (`NROS_HEAP_SIZE`).
const fn parse_usize(s: &str) -> usize {
    let b = s.as_bytes();
    let mut v = 0usize;
    let mut i = 0;
    while i < b.len() {
        assert!(
            b[i] >= b'0' && b[i] <= b'9',
            "NROS_ZEPHYR_HEAP_SIZE must be decimal bytes"
        );
        v = v * 10 + (b[i] - b'0') as usize;
        i += 1;
    }
    v
}

static HEAP: FreeListHeap<HEAP_SIZE> = FreeListHeap::new();

/// The C funnel's backing. Callers hold the funnel's `k_spinlock`.
#[unsafe(no_mangle)]
pub extern "C" fn nros_zephyr_heap_alloc(size: usize) -> *mut core::ffi::c_void {
    HEAP.alloc(size)
}

/// See [`nros_zephyr_heap_alloc`].
#[unsafe(no_mangle)]
pub extern "C" fn nros_zephyr_heap_realloc(
    ptr: *mut core::ffi::c_void,
    size: usize,
) -> *mut core::ffi::c_void {
    HEAP.realloc(ptr, size)
}

/// See [`nros_zephyr_heap_alloc`].
#[unsafe(no_mangle)]
pub extern "C" fn nros_zephyr_heap_free(ptr: *mut core::ffi::c_void) {
    HEAP.free(ptr)
}

/// Managed capacity, for the platform's unified-heap figures.
#[unsafe(no_mangle)]
pub extern "C" fn nros_zephyr_heap_capacity() -> usize {
    HEAP.capacity()
}

/// phase-412 -- bytes this arena currently has handed out.
///
/// `FreeListHeap` has tracked this all along; nothing exported it, so the
/// platform's `nros_platform_heap_used_bytes()` answered from Zephyr's
/// `_system_heap` instead -- the heap the application STOPPED allocating from
/// when the funnel moved here (phase-391 W3). The figure that would size
/// `NROS_ZEPHYR_HEAP_SIZE` was measuring a different arena.
#[unsafe(no_mangle)]
pub extern "C" fn nros_zephyr_heap_used() -> usize {
    HEAP.used()
}

/// The most this arena has ever had handed out at once.
///
/// This is the number `NROS_ZEPHYR_HEAP_SIZE` should be set from, plus
/// headroom. `used` sampled at an arbitrary instant reports whatever happened
/// to be live at the moment of the read, which is not what the knob bounds.
#[unsafe(no_mangle)]
pub extern "C" fn nros_zephyr_heap_peak() -> usize {
    HEAP.peak()
}

/// Issue 1370 — the SHAPE of the free memory, for the exhaustion report:
/// writes the largest contiguous free block and the total free payload of the
/// main arena, both by walking rlsf's block list, and returns 1 when a refused
/// request of `size` bytes is EXTERNAL FRAGMENTATION (the bytes are free, no
/// single hole holds them) and 0 when the arena is simply too small.
///
/// O(blocks), so it is called on the failure path only, under the funnel's
/// spinlock like every other export here.
///
/// # Safety
/// `largest` and `total` must be valid for one `usize` write each.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_zephyr_heap_free_shape(
    size: usize,
    largest: *mut usize,
    total: *mut usize,
) -> i32 {
    let shape = HEAP.free_shape();
    unsafe {
        *largest = shape.largest_free;
        *total = shape.free_total;
    }
    match zpico_alloc::Exhaustion::classify(size, shape) {
        zpico_alloc::Exhaustion::Fragmented => 1,
        zpico_alloc::Exhaustion::TooSmall => 0,
    }
}
