//! Phase 130.3 / 130.5 — `NodeWake`: heap-backed wake primitive
//! used by `Executor::spin_once` on builds where the
//! `nros_platform_wake_*` ABI is wired (Phase 130.1 + 130.5).
//!
//! Originally added for Zephyr+std because Zephyr's libc
//! `pthread_cond_timedwait` hangs past its deadline (Phase
//! 127.C.4). Now also active on FreeRTOS, NuttX, and ThreadX
//! when those platforms are linked — each owns a kernel-native
//! binary semaphore (`k_sem` / `xSemaphoreBinary` / `sem_t` /
//! `tx_semaphore`) that honors its timeout. POSIX-only hosts
//! still use the existing `std::sync::Condvar` path.
//!
//! Construction is fallible — if the platform provider hasn't
//! linked a wake primitive (`wake_storage_size() == 0`) or
//! `wake_init` returns non-zero, the caller falls back to
//! driving the transport for the full timeout (matches the
//! Phase 127.C.4 expedient gate behaviour but without skipping
//! reliable RTOS stream retransmission).
//!
//! Phase 141.A.2 — cfg relaxed from `std` to `alloc` so the same
//! `NodeWake` type works for the embedded no_std FreeRTOS wake-cb
//! path (target for the Cortex-M3 P99 acceptance). The
//! `Box`/`Vec`/`Arc` types switch to the `alloc` crate; the
//! kernel-side semantics are unchanged.

// Phase 248 (C2) — platform-agnostic: no `platform-*` feature gate.
// `NodeWake` calls the `nros_platform_wake_*` C ABI (the platform
// vtable) generically; availability is decided at runtime by the
// `wake_storage_size() == 0` probe in [`NodeWake::new`], not by a
// compile-time per-RTOS cfg. Compiled for any `alloc + rmw-cffi` build;
// platforms without a wake primitive simply report size 0 and the
// caller falls back to driving the transport for the full timeout.
#![cfg(all(feature = "alloc", feature = "rmw-cffi"))]
// Phase 141.A.2 — `NodeWake` is callable from the std-gated
// `install_wake_signal_on_*` path today; the matching no_std
// caller for the FreeRTOS-embedded wake-cb path is the
// follow-on 141.A.3 work. Until that lands, the no_std build of
// this module has no consumer — `#[allow(dead_code)]` keeps the
// type compilable so 141.A.3 only has to add the caller, not
// re-introduce the type.
#![cfg_attr(not(feature = "std"), allow(dead_code))]

use core::ffi::c_void;

unsafe extern "C" {
    fn nros_platform_wake_init(w: *mut c_void) -> i8;
    fn nros_platform_wake_drop(w: *mut c_void) -> i8;
    fn nros_platform_wake_wait_ms(w: *mut c_void, timeout_ms: u32) -> i8;
    fn nros_platform_wake_signal(w: *mut c_void) -> i8;
    #[allow(dead_code)] // Wired by 124.B.7 ISR callers in a follow-up.
    fn nros_platform_wake_signal_from_isr(w: *mut c_void) -> i8;
    fn nros_platform_wake_storage_size() -> usize;
    fn nros_platform_wake_storage_align() -> usize;
}

/// Heap-backed wake primitive. Sized at runtime from the platform
/// probe (`nros_platform_wake_storage_size`).
pub(crate) struct NodeWake {
    storage: alloc::boxed::Box<[u8]>,
}

// SAFETY: per `<nros/platform.h>`'s wake contract, signal/wait are
// callable from any thread (k_sem on Zephyr is kernel-managed).
unsafe impl Send for NodeWake {}
unsafe impl Sync for NodeWake {}

impl NodeWake {
    /// Allocate + init. Returns `None` if the platform provider
    /// reports the primitive unavailable (`storage_size() == 0`)
    /// or if `wake_init` returns non-zero.
    pub(crate) fn new() -> Option<Self> {
        // SAFETY: probe functions are documented pure (no global
        // state, may be called before init).
        let size = unsafe { nros_platform_wake_storage_size() };
        let align = unsafe { nros_platform_wake_storage_align() };
        if size == 0 {
            return None;
        }
        // Round capacity up so the boxed slice's data ptr is
        // aligned to the requested boundary. `Box::<[u8]>` only
        // guarantees byte alignment; layout-allocate via Vec
        // capacity + a manual alignment check, or fall back to
        // `vec![0; size + align]` and offset. Simplest: use a
        // `Vec<u64>` for >=8B alignment (covers every platform we
        // ship today; `k_sem` and `sem_t` are <= 8B-aligned).
        if align > core::mem::align_of::<u64>() {
            return None;
        }
        let u64s = size.div_ceil(8);
        let boxed: alloc::boxed::Box<[u64]> = alloc::vec![0u64; u64s].into_boxed_slice();
        // Reinterpret the boxed [u64] as a boxed [u8]. The
        // capacity in bytes is `u64s * 8 >= size`; the data
        // pointer inherits 8-byte alignment from `Vec<u64>`'s
        // allocator request.
        // Take the data pointer as a THIN `*mut u8` (not `as *mut [u8]`, which
        // would keep the `u64s` element count for a u8 slice — wrong length, and
        // trips `clippy::cast_slice_different_sizes`). The fat slice is rebuilt
        // below with the correct byte length.
        let raw = alloc::boxed::Box::into_raw(boxed) as *mut u8;
        // SAFETY: `raw` came from `Box::<[u64]>::into_raw`; we
        // re-box as `Box<[u8]>` with the same total byte length.
        // The allocator only cares about the total size + the
        // pointer originally returned, both preserved.
        let storage: alloc::boxed::Box<[u8]> = unsafe {
            alloc::boxed::Box::from_raw(core::ptr::slice_from_raw_parts_mut(raw, u64s * 8))
        };
        let ptr = storage.as_ptr() as *mut c_void;
        // SAFETY: `ptr` references at least `size` bytes aligned
        // to 8 (verified above). `wake_init` initialises in place
        // or returns non-zero — in which case we drop `storage`
        // without calling `wake_drop`, matching the "init failed,
        // no teardown" contract.
        let rc = unsafe { nros_platform_wake_init(ptr) };
        if rc != 0 {
            return None;
        }
        Some(Self { storage })
    }

    /// Block until signaled or `timeout_ms` elapses. Returns
    /// `true` on signal, `false` on timeout or error.
    pub(crate) fn wait_ms(&self, timeout_ms: u32) -> bool {
        let ptr = self.storage.as_ptr() as *mut c_void;
        // SAFETY: ptr was init'd in `new`; the underlying
        // primitive supports concurrent calls per platform spec.
        unsafe { nros_platform_wake_wait_ms(ptr, timeout_ms) == 0 }
    }

    /// Wake one waiter. Safe from any thread.
    pub(crate) fn signal(&self) {
        let ptr = self.storage.as_ptr() as *mut c_void;
        // SAFETY: see wait_ms.
        let _ = unsafe { nros_platform_wake_signal(ptr) };
    }
}

impl Drop for NodeWake {
    fn drop(&mut self) {
        let ptr = self.storage.as_ptr() as *mut c_void;
        // SAFETY: ptr was init'd in `new`; this is the matching
        // teardown call. NodeWake is owned exclusively by the
        // Executor (held inside Arc), so no in-flight wait can
        // overlap Drop.
        let _ = unsafe { nros_platform_wake_drop(ptr) };
    }
}
