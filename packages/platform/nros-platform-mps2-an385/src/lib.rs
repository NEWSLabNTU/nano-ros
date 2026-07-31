//! nros platform implementation for QEMU MPS2-AN385 bare-metal.
//!
//! Provides all platform primitives (clock, memory, random, sleep, time,
//! threading stubs, libc stubs) for the MPS2-AN385 board. Used by both
//! zpico-platform-shim and xrce-platform-shim via the nros-platform
//! `ConcretePlatform` type alias.
//!
//! This crate has **no nros dependency** — it only provides the hardware
//! abstraction needed by the unified platform interface.

#![no_std]

pub mod clock;
pub mod libc_stubs;
pub mod memory;
pub mod net;
pub mod random;
pub mod serial;
pub mod sleep;
pub mod sporadic_timer;
pub mod threading;
pub mod time;
pub mod timing;

/// Zero-sized type implementing all platform methods for MPS2-AN385.
///
/// Methods match the signatures expected by zpico-platform-shim and
/// xrce-platform-shim (delegating to the hardware-specific modules).
pub struct Mps2An385Platform;

// Phase 121.2.embedded — canonical C ABI export. See `nros-platform-cffi`.
#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export!(Mps2An385Platform);

// Phase 121.8 — net surface export. Backed by `define_smoltcp_platform!`
// (PlatformTcp/Udp/SocketHelpers/UdpMulticast) + default PlatformNetworkPoll.
#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_net!(Mps2An385Platform);

// Phase 110.E.b — `PlatformTimer` impl. CMSDK Timer1 drives a
// single periodic-callback slot via `sporadic_timer::register_periodic`;
// the `#[interrupt] fn TIMER1` handler invokes the registered
// `atomic_sporadic_refill_thunk` from ISR context. `create_oneshot`
// stays `Unsupported` — the oneshot machinery (per-callback overrun
// detection) needs a second timer source + is design-deferred.
impl nros_platform_api::PlatformTimer for Mps2An385Platform {
    type TimerHandle = TimerHandleStub;

    fn create_periodic(
        period_us: u32,
        callback: extern "C" fn(*mut core::ffi::c_void),
        user_data: *mut core::ffi::c_void,
    ) -> Result<Self::TimerHandle, nros_platform_api::TimerError> {
        sporadic_timer::register_periodic(period_us, callback, user_data)?;
        Ok(TimerHandleStub(1 as *mut core::ffi::c_void))
    }

    fn destroy(_handle: Self::TimerHandle) {
        sporadic_timer::destroy();
    }
}

/// Pointer-sized newtype satisfying `nros_platform_export_timer!`'s
/// `size_of::<TimerHandle>() == size_of::<*mut c_void>()` guard.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct TimerHandleStub(*mut core::ffi::c_void);

// SAFETY: the wrapped pointer is a sentinel only; nothing reads it.
unsafe impl Send for TimerHandleStub {}
unsafe impl Sync for TimerHandleStub {}

#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_timer!(Mps2An385Platform);

// Phase 88 — `PlatformLog` impl + canonical C ABI export. Routes
// log records through QEMU semihosting (`hstderr`). Format:
// `[<LEVEL>] <name>: <msg>` followed by `\n`. Not ISR-safe —
// semihosting BKPT halts the CPU until the debugger services the
// request; calling from an ISR is fine on QEMU but should be
// avoided on real hardware-with-debugger setups.
impl nros_platform_api::PlatformLog for Mps2An385Platform {
    fn write(severity: u8, name: &[u8], message: &[u8]) {
        use core::fmt::Write as _;
        let Ok(mut out) = cortex_m_semihosting::hio::hstderr() else {
            return;
        };
        let label = match severity {
            0 => "TRACE",
            1 => "DEBUG",
            2 => "INFO",
            3 => "WARN",
            4 => "ERROR",
            5 => "FATAL",
            _ => "?",
        };
        let name_str = core::str::from_utf8(name).unwrap_or("");
        let msg_str = core::str::from_utf8(message).unwrap_or("");
        if !name_str.is_empty() {
            let _ = writeln!(out, "[{}] {}: {}", label, name_str, msg_str);
        } else {
            let _ = writeln!(out, "[{}] {}", label, msg_str);
        }
    }
}

#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_log!(Mps2An385Platform);

// Phase 121.9 — Cortex-M PRIMASK critical section. Always emitted
// (independent of the `critical-section` feature, which only gates
// the `critical_section::set_impl!` global registration). The
// canonical `nros_platform_critical_section_{acquire,release}` C
// symbols come from `nros_platform_export!` above via this impl.
impl nros_platform_api::PlatformCriticalSection for Mps2An385Platform {
    fn acquire() -> u32 {
        let was_enabled = cortex_m::register::primask::read().is_active();
        cortex_m::interrupt::disable();
        if was_enabled { 1 } else { 0 }
    }
    fn release(token: u32) {
        if token == 1 {
            // SAFETY: prior posture was "enabled"; outermost release.
            unsafe { cortex_m::interrupt::enable() };
        }
    }
}

// ============================================================================
// Clock
// ============================================================================

impl nros_platform_api::PlatformClock for Mps2An385Platform {
    #[inline]
    fn clock_ms() -> u64 {
        clock::clock_ms()
    }

    #[inline]
    fn clock_us() -> u64 {
        clock::clock_ms() * 1000
    }
}

// ============================================================================
// Memory
// ============================================================================

impl nros_platform_api::PlatformAlloc for Mps2An385Platform {
    #[inline]
    fn alloc(size: usize) -> *mut core::ffi::c_void {
        memory::alloc(size)
    }

    #[inline]
    fn realloc(ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void {
        memory::realloc(ptr, size)
    }

    #[inline]
    fn dealloc(ptr: *mut core::ffi::c_void) {
        memory::dealloc(ptr)
    }

    #[inline]
    fn heap_used_bytes() -> usize {
        memory::used()
    }

    #[inline]
    fn heap_total_bytes() -> usize {
        memory::total()
    }
}

// ============================================================================
// Sleep
// ============================================================================

impl nros_platform_api::PlatformSleep for Mps2An385Platform {
    #[inline]
    fn sleep_us(us: usize) {
        sleep::sleep_ms(us.div_ceil(1000));
    }

    #[inline]
    fn sleep_ms(ms: usize) {
        sleep::sleep_ms(ms);
    }

    #[inline]
    fn sleep_s(s: usize) {
        sleep::sleep_ms(s * 1000);
    }
}

// ============================================================================
// Yield — single-core no-preempt bare-metal: spin_loop hint only
// ============================================================================

impl nros_platform_api::PlatformYield for Mps2An385Platform {
    #[inline]
    fn yield_now() {
        core::hint::spin_loop();
    }
}

// ============================================================================
// Random
// ============================================================================

impl nros_platform_api::PlatformRandom for Mps2An385Platform {
    #[inline]
    fn random_u8() -> u8 {
        random::random_u8()
    }

    #[inline]
    fn random_u16() -> u16 {
        random::random_u16()
    }

    #[inline]
    fn random_u32() -> u32 {
        random::random_u32()
    }

    #[inline]
    fn random_u64() -> u64 {
        random::random_u64()
    }

    #[inline]
    fn random_fill(buf: *mut core::ffi::c_void, len: usize) {
        random::random_fill(buf, len)
    }
}

// ============================================================================
// Time
// ============================================================================

impl nros_platform_api::PlatformTime for Mps2An385Platform {
    #[inline]
    fn time_now_ms() -> u64 {
        clock::clock_ms()
    }

    #[inline]
    fn time_since_epoch_secs() -> u32 {
        (clock::clock_ms() / 1000) as u32
    }

    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        ((clock::clock_ms() % 1000) * 1_000_000) as u32
    }
}

// ============================================================================
// Threading (single-threaded bare-metal — all no-ops)
// ============================================================================

impl Mps2An385Platform {
    pub fn task_init(
        _task: *mut core::ffi::c_void,
        _attr: *mut core::ffi::c_void,
        _entry: Option<unsafe extern "C" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void>,
        _arg: *mut core::ffi::c_void,
    ) -> i8 {
        -1 // Cannot create threads on single-threaded platform
    }

    pub fn task_join(_task: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_detach(_task: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_cancel(_task: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_exit() {}
    pub fn task_free(_task: *mut *mut core::ffi::c_void) {}

    pub fn mutex_init(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_drop(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_lock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_try_lock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_unlock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }

    pub fn mutex_rec_init(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_drop(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_lock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_try_lock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_unlock(_m: *mut core::ffi::c_void) -> i8 {
        0
    }

    pub fn condvar_init(_cv: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_drop(_cv: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_signal(_cv: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_signal_all(_cv: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_wait(_cv: *mut core::ffi::c_void, _m: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_wait_until(
        _cv: *mut core::ffi::c_void,
        _m: *mut core::ffi::c_void,
        _abstime: u64,
    ) -> i8 {
        0
    }
}

impl nros_platform_api::PlatformThreading for Mps2An385Platform {
    fn task_init(
        task: *mut core::ffi::c_void,
        attr: *mut core::ffi::c_void,
        entry: Option<unsafe extern "C" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void>,
        arg: *mut core::ffi::c_void,
    ) -> i8 {
        Self::task_init(task, attr, entry, arg)
    }
    fn task_join(task: *mut core::ffi::c_void) -> i8 {
        Self::task_join(task)
    }
    fn task_detach(task: *mut core::ffi::c_void) -> i8 {
        Self::task_detach(task)
    }
    fn task_cancel(task: *mut core::ffi::c_void) -> i8 {
        Self::task_cancel(task)
    }
    fn task_exit() {
        Self::task_exit()
    }
    fn task_free(task: *mut *mut core::ffi::c_void) {
        Self::task_free(task)
    }
    fn mutex_init(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_init(m)
    }
    fn mutex_drop(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_drop(m)
    }
    fn mutex_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_lock(m)
    }
    fn mutex_try_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_try_lock(m)
    }
    fn mutex_unlock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_unlock(m)
    }
    fn mutex_rec_init(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_init(m)
    }
    fn mutex_rec_drop(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_drop(m)
    }
    fn mutex_rec_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_lock(m)
    }
    fn mutex_rec_try_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_try_lock(m)
    }
    fn mutex_rec_unlock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_unlock(m)
    }
    fn condvar_init(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_init(cv)
    }
    fn condvar_drop(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_drop(cv)
    }
    fn condvar_signal(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_signal(cv)
    }
    fn condvar_signal_all(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_signal_all(cv)
    }
    fn condvar_wait(cv: *mut core::ffi::c_void, m: *mut core::ffi::c_void) -> i8 {
        Self::condvar_wait(cv, m)
    }
    fn condvar_wait_until(
        cv: *mut core::ffi::c_void,
        m: *mut core::ffi::c_void,
        abstime: u64,
    ) -> i8 {
        Self::condvar_wait_until(cv, m, abstime)
    }
}
