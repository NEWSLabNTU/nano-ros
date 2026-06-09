//! Free-list heap allocator for bare-metal MPS2-AN385.
//!
//! Heap size (the `HEAP` static lands in RAM — `.bss`/`.data`):
//!   - 64 KB default (zenoh-pico / xrce-dds builds)
//!   - 128 KB with `link-tls` (mbedTLS context + certs + crypto)
//!   - 2 MB with `dds-heap` (Phase 97.3.mps2-an385 — DDS
//!     DcpsDomainParticipant builtin entities; same budget as the
//!     FreeRTOS / ThreadX-RV64 slices)
//!
//! Phase 204.5 — `NROS_HEAP_SIZE` (decimal bytes, set in the example's
//! `.cargo/config.toml` `[env]`) overrides the default. The defaults are
//! generous; a zenoh-pico `tcp/` client's working set is ~12–16 KB and
//! XRCE's ~3 KB, so a size-critical node can shrink the static heap a lot
//! (e.g. `NROS_HEAP_SIZE = "16384"`). The default is unchanged when the
//! var is unset — no regression for examples that don't set it.

use zpico_alloc::FreeListHeap;

#[cfg(feature = "dds-heap")]
const DEFAULT_HEAP_SIZE: usize = 2 * 1024 * 1024;
#[cfg(all(feature = "link-tls", not(feature = "dds-heap")))]
const DEFAULT_HEAP_SIZE: usize = 128 * 1024;
#[cfg(not(any(feature = "link-tls", feature = "dds-heap")))]
const DEFAULT_HEAP_SIZE: usize = 64 * 1024;

/// `NROS_HEAP_SIZE` (compile-time env, decimal bytes) or [`DEFAULT_HEAP_SIZE`].
const HEAP_SIZE: usize = match option_env!("NROS_HEAP_SIZE") {
    Some(s) => parse_usize(s),
    None => DEFAULT_HEAP_SIZE,
};

/// `const`-evaluable decimal parse — `NROS_HEAP_SIZE` is a build-config
/// literal, so a bad value fails the build rather than at runtime.
const fn parse_usize(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut n: usize = 0;
    while i < bytes.len() {
        let b = bytes[i];
        assert!(b >= b'0' && b <= b'9', "NROS_HEAP_SIZE must be decimal bytes");
        n = n * 10 + (b - b'0') as usize;
        i += 1;
    }
    assert!(n > 0, "NROS_HEAP_SIZE must be > 0");
    n
}

// Issue #6 — opt-in unified heap. With the `global-alloc` feature, the same
// `FreeListHeap` that backs `z_malloc` is also installed as the Rust
// `#[global_allocator]`, so zenoh-pico C allocations and Rust `Box`/`Vec`
// draw from one heap. Without the feature (the default) no global allocator
// is installed and the bare-metal target is unchanged.
#[cfg(feature = "global-alloc")]
#[global_allocator]
static HEAP: FreeListHeap<HEAP_SIZE> = FreeListHeap::new();

#[cfg(not(feature = "global-alloc"))]
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
