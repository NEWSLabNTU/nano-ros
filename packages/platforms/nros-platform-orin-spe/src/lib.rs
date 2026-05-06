//! nros platform for the NVIDIA AGX Orin SPE (Sensor Processing Engine).
//!
//! The SPE runs NVIDIA's FreeRTOS V10.4.3 FSP on a Cortex-R5F core. The
//! tick / xTaskCreate / pvPortMalloc / mutex / condvar API surface is
//! the same as upstream FreeRTOS, so every non-IVC trait impl forwards
//! verbatim to [`nros_platform_freertos::FreeRtosPlatform`].
//!
//! What's distinctive on the SPE:
//!
//! * **Transport.** The SPE has no Ethernet or dedicated UART; the
//!   single TCU is a shared debug multiplexer. The only Linux↔SPE
//!   transport is **IVC** (Inter-VM Communication on Tegra) —
//!   shared-DRAM ring buffers signalled by an HSP doorbell. We
//!   surface it through [`PlatformIvc`](nros_platform_api::PlatformIvc),
//!   delegating to the `nvidia-ivc` driver crate. Pick `fsp` to bind
//!   NVIDIA's `tegra_aon_fsp.a`; pick `unix-mock` for the FreeRTOS
//!   POSIX-simulator dev path.
//!
//! * **Critical section.** ARMv7-R CPSR I-bit, not Cortex-M PRIMASK.
//!   Pull `cortex-r` to register the corresponding
//!   `critical_section::Impl` (forwards to
//!   `nros-platform-freertos`'s `cortex-r` feature added in Phase 100.1).
//!
//! * **No hardware RNG.** Best-effort xorshift32 seeded from the
//!   FreeRTOS tick — adequate for zenoh-pico's SN/scout-jitter
//!   needs, **not** for crypto. See `random.rs` for the precise
//!   contract.

#![cfg_attr(not(feature = "unix-mock"), no_std)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;
use nros_platform_api::{
    PlatformAlloc, PlatformClock, PlatformSleep, PlatformThreading, PlatformTime, PlatformYield,
};
use nros_platform_freertos::FreeRtosPlatform;

mod ivc;
mod random;

/// Zero-sized type that resolves as `nros_platform::ConcretePlatform`
/// when the `platform-orin-spe` feature is enabled on `nros-platform`.
pub struct OrinSpe;

// =============================================================================
// Trait impls — forward to FreeRtosPlatform.
//
// The forwarding is one-liner per method; a `macro_rules!` would buy
// us nothing once the names diverge per trait, and the explicit form
// is easier to grep when a future bring-up needs to peel one impl off
// (e.g., a custom `clock_us` reading the HRT directly instead of the
// FreeRTOS tick). PlatformIvc + PlatformRandom are the two non-trivial
// impls and live in their own modules.
// =============================================================================

impl PlatformClock for OrinSpe {
    #[inline]
    fn clock_ms() -> u64 {
        FreeRtosPlatform::clock_ms()
    }
    #[inline]
    fn clock_us() -> u64 {
        FreeRtosPlatform::clock_us()
    }
}

impl PlatformAlloc for OrinSpe {
    #[inline]
    fn alloc(size: usize) -> *mut c_void {
        FreeRtosPlatform::alloc(size)
    }
    #[inline]
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        FreeRtosPlatform::realloc(ptr, size)
    }
    #[inline]
    fn dealloc(ptr: *mut c_void) {
        FreeRtosPlatform::dealloc(ptr)
    }
}

impl PlatformSleep for OrinSpe {
    #[inline]
    fn sleep_us(us: usize) {
        FreeRtosPlatform::sleep_us(us)
    }
    #[inline]
    fn sleep_ms(ms: usize) {
        FreeRtosPlatform::sleep_ms(ms)
    }
    #[inline]
    fn sleep_s(s: usize) {
        FreeRtosPlatform::sleep_s(s)
    }
}

impl PlatformYield for OrinSpe {
    #[inline]
    fn yield_now() {
        FreeRtosPlatform::yield_now()
    }
}

// Phase 110.D — Orin SPE runs on NVIDIA's FreeRTOS FSP (Cortex-R5F).
// Forward every `PlatformScheduler` entry point to the FreeRTOS
// impl; the priority direction (high-numeric = high) and per-thread
// `vTaskPrioritySet` semantics are identical.
impl nros_platform_api::PlatformScheduler for OrinSpe {
    #[inline]
    fn set_current_thread_policy(
        p: nros_platform_api::SchedPolicy,
    ) -> Result<(), nros_platform_api::SchedError> {
        <FreeRtosPlatform as nros_platform_api::PlatformScheduler>::set_current_thread_policy(p)
    }

    #[inline]
    fn yield_now() {
        <FreeRtosPlatform as nros_platform_api::PlatformScheduler>::yield_now()
    }

    #[inline]
    fn set_affinity(cpu_mask: u32) -> Result<(), nros_platform_api::SchedError> {
        <FreeRtosPlatform as nros_platform_api::PlatformScheduler>::set_affinity(cpu_mask)
    }
}

impl PlatformTime for OrinSpe {
    #[inline]
    fn time_now_ms() -> u64 {
        FreeRtosPlatform::time_now_ms()
    }
    #[inline]
    fn time_since_epoch_secs() -> u32 {
        FreeRtosPlatform::time_since_epoch_secs()
    }
    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        FreeRtosPlatform::time_since_epoch_nanos()
    }
}

impl PlatformThreading for OrinSpe {
    #[inline]
    fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        FreeRtosPlatform::task_init(task, attr, entry, arg)
    }
    #[inline]
    fn task_join(task: *mut c_void) -> i8 {
        FreeRtosPlatform::task_join(task)
    }
    #[inline]
    fn task_detach(task: *mut c_void) -> i8 {
        FreeRtosPlatform::task_detach(task)
    }
    #[inline]
    fn task_cancel(task: *mut c_void) -> i8 {
        FreeRtosPlatform::task_cancel(task)
    }
    #[inline]
    fn task_exit() {
        FreeRtosPlatform::task_exit()
    }
    #[inline]
    fn task_free(task: *mut *mut c_void) {
        FreeRtosPlatform::task_free(task)
    }

    #[inline]
    fn mutex_init(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_init(m)
    }
    #[inline]
    fn mutex_drop(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_drop(m)
    }
    #[inline]
    fn mutex_lock(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_lock(m)
    }
    #[inline]
    fn mutex_try_lock(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_try_lock(m)
    }
    #[inline]
    fn mutex_unlock(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_unlock(m)
    }

    #[inline]
    fn mutex_rec_init(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_rec_init(m)
    }
    #[inline]
    fn mutex_rec_drop(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_rec_drop(m)
    }
    #[inline]
    fn mutex_rec_lock(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_rec_lock(m)
    }
    #[inline]
    fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_rec_try_lock(m)
    }
    #[inline]
    fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        FreeRtosPlatform::mutex_rec_unlock(m)
    }

    #[inline]
    fn condvar_init(cv: *mut c_void) -> i8 {
        FreeRtosPlatform::condvar_init(cv)
    }
    #[inline]
    fn condvar_drop(cv: *mut c_void) -> i8 {
        FreeRtosPlatform::condvar_drop(cv)
    }
    #[inline]
    fn condvar_signal(cv: *mut c_void) -> i8 {
        FreeRtosPlatform::condvar_signal(cv)
    }
    #[inline]
    fn condvar_signal_all(cv: *mut c_void) -> i8 {
        FreeRtosPlatform::condvar_signal_all(cv)
    }
    #[inline]
    fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        FreeRtosPlatform::condvar_wait(cv, m)
    }
    #[inline]
    fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        FreeRtosPlatform::condvar_wait_until(cv, m, abstime)
    }
}

// `PlatformRandom` lives in `random.rs` — best-effort xorshift32 seeded
// from the tick. The default forward-to-FreeRtosPlatform impl would
// give us the same shape, but the SPE has no hardware RNG and we want
// to surface that limitation in one place rather than letting it
// silently inherit from a sibling crate that may grow a stronger impl.
//
// `PlatformIvc` lives in `ivc.rs` — delegates to the `nvidia-ivc`
// driver crate.

// =============================================================================
// Net sizes — SPE has no TCP/UDP, but `nros-platform`'s resolver still
// re-exports these constants from the platform module. We surface the
// 64-byte fallback used by other no-network platforms (mps2-an385,
// stm32f4) so the umbrella crate can route the import without any
// special-casing of `platform-orin-spe`.
// =============================================================================

pub const NET_SOCKET_SIZE: usize = 64;
pub const NET_SOCKET_ALIGN: usize = 8;
pub const NET_ENDPOINT_SIZE: usize = 64;
pub const NET_ENDPOINT_ALIGN: usize = 8;
