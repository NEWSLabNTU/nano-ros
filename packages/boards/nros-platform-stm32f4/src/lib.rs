//! nros platform implementation for STM32F4 bare-metal.
//!
//! Uses the DWT cycle counter for timing. The board crate must call
//! `clock::init(sysclk_hz)` and periodically call `clock::update_from_dwt()`.

#![no_std]

pub mod clock;
pub mod libc_stubs;
pub mod memory;
pub mod phy;
pub mod pins;
pub mod random;
pub mod sleep;
pub mod timing;

/// Zero-sized type implementing all platform methods for STM32F4.
pub struct Stm32f4Platform;

impl Stm32f4Platform {
    #[inline]
    pub fn clock_ms() -> u64 { clock::clock_ms() }
    #[inline]
    pub fn clock_us() -> u64 { clock::clock_ms() * 1000 }

    #[inline]
    pub fn alloc(size: usize) -> *mut core::ffi::c_void { memory::alloc(size) }
    #[inline]
    pub fn realloc(ptr: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void { memory::realloc(ptr, size) }
    #[inline]
    pub fn dealloc(ptr: *mut core::ffi::c_void) { memory::dealloc(ptr) }

    #[inline]
    pub fn sleep_us(us: usize) { sleep::sleep_ms(us.div_ceil(1000)); }
    #[inline]
    pub fn sleep_ms(ms: usize) { sleep::sleep_ms(ms); }
    #[inline]
    pub fn sleep_s(s: usize) { sleep::sleep_ms(s * 1000); }

    #[inline]
    pub fn random_u8() -> u8 { random::random_u8() }
    #[inline]
    pub fn random_u16() -> u16 { random::random_u16() }
    #[inline]
    pub fn random_u32() -> u32 { random::random_u32() }
    #[inline]
    pub fn random_u64() -> u64 { random::random_u64() }
    #[inline]
    pub fn random_fill(buf: *mut core::ffi::c_void, len: usize) { random::random_fill(buf, len) }

    #[inline]
    pub fn time_now_ms() -> u64 { clock::clock_ms() }
    #[inline]
    pub fn time_since_epoch_secs() -> u32 { (clock::clock_ms() / 1000) as u32 }
    #[inline]
    pub fn time_since_epoch_nanos() -> u32 { ((clock::clock_ms() % 1000) * 1_000_000) as u32 }

    // Threading — single-threaded bare-metal, all no-ops
    pub fn task_init(_: *mut core::ffi::c_void, _: *mut core::ffi::c_void, _: Option<unsafe extern "C" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void>, _: *mut core::ffi::c_void) -> i8 { -1 }
    pub fn task_join(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn task_detach(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn task_cancel(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn task_exit() {}
    pub fn task_free(_: *mut *mut core::ffi::c_void) {}
    pub fn mutex_init(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_drop(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_lock(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_try_lock(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_unlock(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_rec_init(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_rec_drop(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_rec_lock(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_rec_try_lock(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn mutex_rec_unlock(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn condvar_init(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn condvar_drop(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn condvar_signal(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn condvar_signal_all(_: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn condvar_wait(_: *mut core::ffi::c_void, _: *mut core::ffi::c_void) -> i8 { 0 }
    pub fn condvar_wait_until(_: *mut core::ffi::c_void, _: *mut core::ffi::c_void, _: u64) -> i8 { 0 }
}
