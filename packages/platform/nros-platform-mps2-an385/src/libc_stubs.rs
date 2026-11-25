//! Heap libc shims (`malloc` / `free` / `realloc` / `calloc`) for bare-metal
//! MPS2-AN385 (XRCE's nano-ros wrapper needs them).
//!
//! Routes through the same `FreeListHeap` zenoh-pico uses (`crate::memory`,
//! sized by `NROS_MEMORY_HEAP_BYTES`), so XRCE inherits the same tunable pool — no
//! second heap.
//!
//! String helpers (`strrchr`, `strtol`, plus the pre-existing
//! `strlen` / `memcpy` / `memset` / `strchr` / `strcmp` / …) live in
//! `nros-baremetal-common::libc_stubs`, which this crate enables via the
//! `libc-stubs` feature.
//!
//! Emitted only on `target_os = "none"`, behind the `libc-heap` feature: on a
//! host build these names would replace the host C library's allocator for
//! the whole process.

// Issue 2932 -- this header used to say the platform is bare-metal-only, so
// there is no hosted libc to collide with. The IMAGE is, but the crate is also
// built for the host: its own unit tests (`just check bare-metal-wall-clock`)
// and host clippy. There these `no_mangle` names replaced glibc's allocator for
// the whole test process, every thread of libtest included, with
// `FreeListHeap`, whose `Sync` impl assumes one context. A race corrupted the
// TLSF bitmap and aborted a gate ("bitmap contradiction").
// `check-host-libc-symbols` keeps every host build of a bare-metal crate free
// of libc names.

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
