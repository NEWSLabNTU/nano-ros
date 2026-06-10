//! nros platform implementation for STM32F4 bare-metal.
//!
//! Uses the DWT cycle counter for timing. The board crate must call
//! `clock::init(sysclk_hz)` and periodically call `clock::update_from_dwt()`.

#![no_std]

pub mod clock;
pub mod libc_stubs;
pub mod memory;
pub mod net;
pub mod phy;
pub mod pins;
pub mod random;
pub mod serial;
pub mod sleep;
pub mod sporadic_timer;
pub mod timing;

/// Zero-sized type implementing all platform methods for STM32F4.
pub struct Stm32f4Platform;

// Phase 121.2.embedded — canonical C ABI export. See `nros-platform-cffi`.
#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export!(Stm32f4Platform);

// Phase 121.8 — net surface export.
#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_net!(Stm32f4Platform);

// Phase 110.E.b — `PlatformTimer` dispatches through the
// per-board periodic-timer hook in `sporadic_timer`. Board crates
// (or user-init code) call
// `sporadic_timer::install_periodic_timer_hook(register, destroy)`
// to wire a TIM2/TIM3/TIM5 IRQ; without a hook installed the impl
// returns `TimerError::Unsupported` and `nros_platform_timer_*`
// surfaces NULL so cross-platform code degrades gracefully. See
// `sporadic_timer` module docs for the hook contract; mps2-an385
// is the canonical drive-the-timer-directly reference.
impl nros_platform_api::PlatformTimer for Stm32f4Platform {
    type TimerHandle = TimerHandleStub;

    fn create_periodic(
        period_us: u32,
        callback: extern "C" fn(*mut core::ffi::c_void),
        user_data: *mut core::ffi::c_void,
    ) -> Result<Self::TimerHandle, nros_platform_api::TimerError> {
        sporadic_timer::dispatch_register(period_us, callback, user_data)?;
        // v1: single-slot hook, sentinel handle (1) — `destroy`
        // ignores the value + tears the hook's slot down.
        Ok(TimerHandleStub(1 as *mut core::ffi::c_void))
    }

    fn destroy(_handle: Self::TimerHandle) {
        sporadic_timer::dispatch_destroy();
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct TimerHandleStub(*mut core::ffi::c_void);
unsafe impl Send for TimerHandleStub {}
unsafe impl Sync for TimerHandleStub {}

#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_timer!(Stm32f4Platform);

// Phase 88 — `PlatformLog` impl + canonical C ABI export. Routes
// log records through `defmt` over RTT (already the board's
// existing logging path; the impl wraps the message body as the
// `"{=str}"` arg so defmt interns the format string once). ISR-safe
// under the standard `critical-section` defmt impl.
impl nros_platform_api::PlatformLog for Stm32f4Platform {
    fn write(severity: u8, name: &[u8], message: &[u8]) {
        // defmt wants a `&str`. Lossy decode is fine — the facade
        // formats from `core::fmt::Arguments`, so the body is UTF-8
        // by construction.
        let name_str = core::str::from_utf8(name).unwrap_or("");
        let msg_str = core::str::from_utf8(message).unwrap_or("");
        match severity {
            0 => defmt::trace!("[nros:{=str}] {=str}", name_str, msg_str),
            1 => defmt::debug!("[nros:{=str}] {=str}", name_str, msg_str),
            2 => defmt::info!("[nros:{=str}] {=str}", name_str, msg_str),
            3 => defmt::warn!("[nros:{=str}] {=str}", name_str, msg_str),
            // defmt has no FATAL — fold into ERROR.
            4 | 5 => defmt::error!("[nros:{=str}] {=str}", name_str, msg_str),
            _ => defmt::info!("[nros:{=str}] {=str}", name_str, msg_str),
        }
    }
}

#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_log!(Stm32f4Platform);

// Phase 121.9 — Cortex-M PRIMASK critical section. See sibling
// mps2-an385 for rationale.
impl nros_platform_api::PlatformCriticalSection for Stm32f4Platform {
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

impl nros_platform_api::PlatformYield for Stm32f4Platform {
    #[inline]
    fn yield_now() {
        core::hint::spin_loop();
    }
}

impl nros_platform_api::PlatformClock for Stm32f4Platform {
    #[inline]
    fn clock_ms() -> u64 {
        clock::clock_ms()
    }
    #[inline]
    fn clock_us() -> u64 {
        clock::clock_ms() * 1000
    }
}

impl nros_platform_api::PlatformAlloc for Stm32f4Platform {
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

impl nros_platform_api::PlatformSleep for Stm32f4Platform {
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

impl nros_platform_api::PlatformRandom for Stm32f4Platform {
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

impl nros_platform_api::PlatformTime for Stm32f4Platform {
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

impl Stm32f4Platform {
    // Threading — single-threaded bare-metal, all no-ops
    pub fn task_init(
        _: *mut core::ffi::c_void,
        _: *mut core::ffi::c_void,
        _: Option<unsafe extern "C" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void>,
        _: *mut core::ffi::c_void,
    ) -> i8 {
        -1
    }
    pub fn task_join(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_detach(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_cancel(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn task_exit() {}
    pub fn task_free(_: *mut *mut core::ffi::c_void) {}
    pub fn mutex_init(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_drop(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_lock(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_try_lock(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_unlock(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_init(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_drop(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_lock(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_try_lock(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn mutex_rec_unlock(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_init(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_drop(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_signal(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_signal_all(_: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_wait(_: *mut core::ffi::c_void, _: *mut core::ffi::c_void) -> i8 {
        0
    }
    pub fn condvar_wait_until(_: *mut core::ffi::c_void, _: *mut core::ffi::c_void, _: u64) -> i8 {
        0
    }
}

impl nros_platform_api::PlatformThreading for Stm32f4Platform {
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
