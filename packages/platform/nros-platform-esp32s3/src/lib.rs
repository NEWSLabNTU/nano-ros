//! nros platform implementation for ESP32-S3 (Xtensa LX7) bare-metal.
//!
//! Uses `esp_hal::time::Instant` for the hardware timer clock. Adapted
//! from the ESP32-C3 platform; the only chip-specific divergence is the
//! `PlatformCriticalSection` impl (Xtensa `rsil`/`wsr.ps` instead of the
//! RISC-V `mstatus` CSR). Inline asm on Xtensa is still feature-gated.

#![no_std]
#![feature(asm_experimental_arch)]

pub mod clock;
pub mod libc_stubs;
pub mod memory;
pub mod net;
pub mod random;
pub mod sleep;
pub mod sporadic_timer;
pub mod timing;

/// Zero-sized type implementing all platform methods for ESP32-C3 QEMU.
pub struct Esp32S3Platform;

// Phase 121.2.embedded — canonical C ABI export. See `nros-platform-cffi`.
#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export!(Esp32S3Platform);

// Phase 121.8 — net surface export.
#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_net!(Esp32S3Platform);

// Phase 110.E.b — `PlatformTimer` dispatches through the
// per-board periodic-timer hook in `sporadic_timer`. Identical
// shape to the `nros-platform-esp32-qemu` sibling.
impl nros_platform_api::PlatformTimer for Esp32S3Platform {
    type TimerHandle = TimerHandleStub;

    fn create_periodic(
        period_us: u32,
        callback: extern "C" fn(*mut core::ffi::c_void),
        user_data: *mut core::ffi::c_void,
    ) -> Result<Self::TimerHandle, nros_platform_api::TimerError> {
        sporadic_timer::dispatch_register(period_us, callback, user_data)?;
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
nros_platform_cffi::nros_platform_export_timer!(Esp32S3Platform);

// Phase 88 — `PlatformLog` for ESP32-C3 QEMU bare-metal. Same
// fn-ptr shape as `nros-platform-esp32-qemu`: board crate (or QEMU test
// harness) registers a writer at startup. No writer = no-op.
mod log_slot {
    use core::sync::atomic::{AtomicUsize, Ordering};
    pub(super) type Writer = fn(severity: u8, name: &[u8], message: &[u8]);

    static WRITER: AtomicUsize = AtomicUsize::new(0);

    pub(super) fn set(writer: Option<Writer>) {
        let raw = match writer {
            Some(w) => w as usize,
            None => 0,
        };
        WRITER.store(raw, Ordering::Release);
    }

    pub(super) fn get() -> Option<Writer> {
        let raw = WRITER.load(Ordering::Acquire);
        if raw == 0 {
            None
        } else {
            // SAFETY: only `set` writes here, and it only stores fn
            // pointers we own.
            Some(unsafe { core::mem::transmute::<usize, Writer>(raw) })
        }
    }
}

/// Register a board-supplied log writer for [`Esp32S3Platform`].
pub fn register_log_writer(writer: Option<log_slot::Writer>) {
    log_slot::set(writer);
}

impl nros_platform_api::PlatformLog for Esp32S3Platform {
    fn write(severity: u8, name: &[u8], message: &[u8]) {
        if let Some(writer) = log_slot::get() {
            writer(severity, name, message);
        }
    }
}

#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_log!(Esp32S3Platform);

// Phase 173.6 — Xtensa critical section. `rsil` reads PS and raises the
// interrupt level to 15 (mask all maskable interrupts), returning the
// prior PS; `wsr.ps` + `rsync` restores it. Replaces the C3's RISC-V
// `mstatus.MIE` CSR sequence. The full prior PS is the token.
impl nros_platform_api::PlatformCriticalSection for Esp32S3Platform {
    fn acquire() -> u32 {
        let prior: u32;
        unsafe {
            core::arch::asm!(
                "rsil {0}, 15",
                out(reg) prior,
                options(nomem, nostack, preserves_flags),
            );
        }
        prior
    }
    fn release(token: u32) {
        unsafe {
            core::arch::asm!(
                "wsr.ps {0}",
                "rsync",
                in(reg) token,
                options(nomem, nostack),
            );
        }
    }
}

impl nros_platform_api::PlatformYield for Esp32S3Platform {
    #[inline]
    fn yield_now() {
        core::hint::spin_loop();
    }
}

impl nros_platform_api::PlatformClock for Esp32S3Platform {
    #[inline]
    fn clock_ms() -> u64 {
        clock::clock_ms()
    }
    #[inline]
    fn clock_us() -> u64 {
        clock::clock_us()
    }
}

impl nros_platform_api::PlatformAlloc for Esp32S3Platform {
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
}

impl nros_platform_api::PlatformSleep for Esp32S3Platform {
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

impl nros_platform_api::PlatformRandom for Esp32S3Platform {
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

impl nros_platform_api::PlatformTime for Esp32S3Platform {
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

impl Esp32S3Platform {
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

impl nros_platform_api::PlatformThreading for Esp32S3Platform {
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
