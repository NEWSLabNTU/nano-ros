//! Busy-wait sleep — delegates to `nros_baremetal_common::sleep`.
//!
//! The platform's init code registers `clock::clock_ms` as the sleep
//! clock source via `nros_baremetal_common::sleep::set_clock_fn`.
//! Once that's done, calls to `sleep_ms` here forward to the shared
//! module which polls the registered clock in a busy-wait loop.

pub use nros_baremetal_common::sleep::{
    IdleFn, PollFn, clear_idle_callback, clear_poll_callback, set_idle_callback,
    set_poll_callback, sleep_ms,
};

/// Register the platform's clock function with the shared sleep
/// module. Must be called once at platform init before any
/// `sleep_ms` call.
pub fn init_clock() {
    nros_baremetal_common::sleep::set_clock_fn(crate::clock::clock_ms);
}

/// Phase 127.D — `wfi` shim used by [`enable_wfi_idle`].
///
/// Wrapped in `extern "C"` so it matches `IdleFn`. SAFETY: `wfi` is
/// only safe once an IRQ source is armed; see [`enable_wfi_idle`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_mps2_an385_wfi_idle() {
    cortex_m::asm::wfi();
}

/// Phase 127.D — install `cortex_m::asm::wfi()` as the sleep-loop idle
/// callback so busy-wait `sleep_ms` invocations yield the CPU to
/// QEMU's main loop (and to other guests sharing the host) between
/// poll-callback iterations.
///
/// **Must only be called AFTER an IRQ source is armed.** RTIC examples
/// should call this immediately after `Mono::start(cx.core.SYST, ..)`.
/// Calling it before any IRQ is armed will deadlock at the first
/// `sleep_ms`/`Executor::open` because `wfi` has nothing to wake on.
///
/// Pre-fix, the busy-wait sleep loop never released CPU; with two
/// MPS2 QEMU instances running in parallel under `-icount shift=auto`
/// the second instance's `Executor::open` consumed 100% of its
/// virtual-time budget on `SmoltcpBridge::poll_network` and never let
/// QEMU's slirp main loop deliver SYN-ACK, which surfaced as
/// `Transport(ConnectionFailed)` (Phase 127.D.1/D.2).
pub fn enable_wfi_idle() {
    set_idle_callback(nros_mps2_an385_wfi_idle);
}
