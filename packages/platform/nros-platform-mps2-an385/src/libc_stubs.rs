//! Phase 207.3.bm-libc — heap libc shims (`malloc` / `free` / `realloc` /
//! `calloc`) for bare-metal MPS2-AN385 (XRCE's nano-ros wrapper needs them).
//!
//! Routes through the same `FreeListHeap` zenoh-pico uses (`crate::memory`,
//! sized by `NROS_HEAP_SIZE` per Phase 204.5), so XRCE inherits the same
//! tunable pool — no second heap.
//!
//! String helpers (`strrchr`, `strtol`, plus the pre-existing
//! `strlen` / `memcpy` / `memset` / `strchr` / `strcmp` / …) live in
//! `nros-baremetal-common::libc_stubs`, which this crate enables via the
//! `libc-stubs` feature.
//!
//! Always-emit (no Cargo feature gate): the mps2-an385 platform is
//! bare-metal-only (`target_os = "none"`), so there's no hosted libc to
//! collide with; emitting these symbols on every build keeps the surface
//! predictable + lets a zenoh build use them too (zenoh-pico's
//! `Z_MALLOC_FUNCTION` already routes via `crate::memory`, but linking
//! `malloc` doesn't pull anything new from zenoh-pico — it stays gc'd
//! when unreferenced).

#![allow(clippy::missing_safety_doc)]

use core::ffi::c_void;

use crate::memory;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    memory::alloc(size)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if !ptr.is_null() {
        memory::dealloc(ptr)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    memory::realloc(ptr, size)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut c_void {
    let total = nmemb.saturating_mul(size);
    let p = memory::alloc(total);
    if !p.is_null() {
        // SAFETY: just allocated `total` bytes from the heap; the region
        // is exclusive to this call until handed back to the caller.
        unsafe { core::ptr::write_bytes(p.cast::<u8>(), 0, total) }
    }
    p
}
