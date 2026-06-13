//! Phase 141.A.2 — no_std variant of [`WakeCtx`] and the runtime
//! wake callback `nros_rmw_runtime_wake_cb`.
//!
//! The std path (`spin.rs::WakeCtx` +
//! `spin.rs::nros_rmw_runtime_wake_cb`) uses `std::sync::Arc` /
//! `std::sync::Condvar` / `std::sync::Mutex` for cross-thread
//! signalling. On RTOS-embedded no_std builds (FreeRTOS / Zephyr /
//! NuttX / ThreadX without std), `std::sync` isn't available, but
//! the platform-cffi `nros_platform_wake_*` ABI exposes a
//! kernel-native binary semaphore (`xSemaphoreBinary` / `k_sem` /
//! `sem_t` / `tx_semaphore`) that satisfies the same wait/signal
//! contract.
//!
//! This module's [`WakeCtxAlloc`] is the no_std mirror of
//! `WakeCtx`: an `Arc<AtomicBool>` for the wake flag and an
//! `Arc<NodeWake>` for the kernel-side wait primitive.
//! [`nros_rmw_runtime_wake_cb`] does the same flag.store +
//! signal sequence as the std cb. Caller wires the `*const
//! WakeCtxAlloc` into a backend session via
//! `set_wake_callback(Some(cb), ctx)`.
//!
//! Phase 141.A.3 will add the `install_wake_signal_on_*_alloc`
//! Executor methods that lazily construct the `Arc<WakeCtxAlloc>`
//! and hand its raw pointer to the active session. Until then,
//! the type compiles and tests against the alloc-side API but
//! has no in-tree caller — `#[allow(dead_code)]` keeps the
//! lint quiet.

// Phase 248 (C2) — platform-agnostic: no `platform-*` feature gate.
// The no_std wake mirror routes through the `nros_platform_wake_*` C
// ABI (platform vtable) generically; the runtime probe in `NodeWake`
// decides availability. Compiled for any no_std `alloc + rmw-cffi`
// build.
#![cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
#![allow(dead_code)]

use portable_atomic::{AtomicBool, Ordering};
use portable_atomic_util::Arc;

use super::node_wake::NodeWake;

/// no_std mirror of [`super::spin::WakeCtx`].
///
/// Fields:
///
/// * `flag` — set by the runtime cb; observed by `spin_once`'s
///   wait predicate. Lock-free; SeqCst on store + acquire on the
///   waiter side.
/// * `node_wake` — kernel-native wake primitive (RTOS semaphore).
///   Calling `signal()` releases any thread blocked in
///   `wait_ms()`. Allocated once per Executor, lives behind an
///   `Arc` so the runtime cb's `*const WakeCtxAlloc` stays valid
///   for the Executor's lifetime.
pub(crate) struct WakeCtxAlloc {
    pub(crate) flag: Arc<AtomicBool>,
    pub(crate) node_wake: Arc<NodeWake>,
}

/// Phase 141.A.2 — runtime wake callback (no_std variant).
///
/// Contract is identical to the std `nros_rmw_runtime_wake_cb`:
///
/// * Thread-safe — `flag.store` is SeqCst; `node_wake.signal`
///   wraps the kernel semaphore's give/post call which is
///   thread-safe per platform spec.
/// * Bounded execution time — O(1): one atomic store + one
///   kernel-call into the wake primitive.
/// * ISR-safety — NOT ISR-safe in this form. ISR callers must
///   route through the platform-cffi
///   `nros_platform_wake_signal_from_isr` slot instead. Same
///   policy as the std variant.
///
/// `ctx` must be a `*const WakeCtxAlloc` obtained from
/// `Arc::into_raw` (or equivalently `Arc::as_ptr` for the
/// duration the Arc is alive).
pub(crate) unsafe extern "C" fn nros_rmw_runtime_wake_cb(ctx: *mut core::ffi::c_void) {
    if ctx.is_null() {
        return;
    }
    // Phase 141.B.2 — capture T0 at cb entry. No-op when the
    // probe feature is off or no cycle reader is installed.
    #[cfg(feature = "wake-latency-probe")]
    super::wake_probe::on_wake();
    // SAFETY: ctx points at a WakeCtxAlloc owned by an Executor
    // still alive at the time of the call. The Executor clears
    // the cb via `set_wake_callback(None, _)` on every session
    // before dropping the Arc; this happens in
    // `install_wake_signal_on_*_alloc` teardown (Phase 141.A.3).
    let wake = unsafe { &*(ctx as *const WakeCtxAlloc) };
    wake.flag.store(true, Ordering::SeqCst);
    wake.node_wake.signal();
}
