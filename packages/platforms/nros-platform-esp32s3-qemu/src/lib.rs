//! nros platform implementation for ESP32-S3 QEMU bare-metal
//! (Phase 117).
//!
//! Mirrors `nros-platform-esp32-qemu` (RISC-V / ESP32-C3) on
//! the Xtensa LX7 / ESP32-S3 side. Two structural differences from
//! the C3 platform:
//!
//! 1. **Critical section uses Xtensa `rsil` / `wsr.ps`**, not
//!    RISC-V `mstatus.MIE`. PS.INTLEVEL is the canonical
//!    interrupt-disable knob on Xtensa; `rsil dst, 15` sets it to
//!    the maximum (kernel-internal) level and atomically returns
//!    the prior PS value, which we hand back to `release` as the
//!    restore token. esp-hal's own `critical-section` impl uses
//!    the same instructions.
//! 2. **Heap budget defaults to 1 MiB** under `dds-heap` instead of
//!    the C3 path's 192 KiB — PSRAM (up to 16 MiB octal) covers
//!    dust-dds's builtin entities + history caches without
//!    cramping internal SRAM. Stack + small fast allocations stay
//!    in SRAM; the board crate routes the heap region to PSRAM
//!    via `esp-alloc`'s region API.
//!
//! Uses `esp_hal::time::Instant` for the hardware timer clock —
//! same shape as C3, just on the LX7 instruction set.

#![no_std]
// Xtensa inline asm is still nightly-only (rust-lang/rust#93335).
// `rsil` / `wsr.ps` in the critical-section impl require this gate.
#![feature(asm_experimental_arch)]

pub mod clock;
pub mod libc_stubs;
pub mod memory;
pub mod net;
pub mod random;
pub mod sleep;
pub mod timing;

/// Zero-sized type implementing all platform methods for ESP32-S3 QEMU.
pub struct Esp32s3QemuPlatform;

// Phase 121.2.embedded — canonical C ABI export.
#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export!(Esp32s3QemuPlatform);

// Phase 121.8 — net surface export.
#[cfg(feature = "cffi-export")]
nros_platform_cffi::nros_platform_export_net!(Esp32s3QemuPlatform);

// Phase 117.1 — Xtensa PS.INTLEVEL critical section. Equivalent to
// the C3 platform's `mstatus.MIE` shape but on the Xtensa
// instruction set. `rsil` reads PS into a register and atomically
// sets PS.INTLEVEL to the given level (15 = mask everything below
// kernel-internal). `wsr.ps + rsync` restores the prior PS value.
impl nros_platform_api::PlatformCriticalSection for Esp32s3QemuPlatform {
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
                options(nomem, nostack, preserves_flags),
            );
        }
    }
}

impl nros_platform_api::PlatformYield for Esp32s3QemuPlatform {
    #[inline]
    fn yield_now() {
        core::hint::spin_loop();
    }
}

impl nros_platform_api::PlatformClock for Esp32s3QemuPlatform {
    #[inline]
    fn clock_ms() -> u64 {
        clock::clock_ms()
    }
    #[inline]
    fn clock_us() -> u64 {
        clock::clock_us()
    }
}

impl nros_platform_api::PlatformAlloc for Esp32s3QemuPlatform {
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

impl nros_platform_api::PlatformSleep for Esp32s3QemuPlatform {
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

impl nros_platform_api::PlatformRandom for Esp32s3QemuPlatform {
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

impl nros_platform_api::PlatformTime for Esp32s3QemuPlatform {
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

impl Esp32s3QemuPlatform {
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

impl nros_platform_api::PlatformThreading for Esp32s3QemuPlatform {
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
