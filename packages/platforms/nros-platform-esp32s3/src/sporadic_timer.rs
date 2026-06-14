//! Phase 110.E.b — periodic-callback hook for ESP32-C3 QEMU
//! sporadic-server budget refill. Same shape as the
//! `nros-platform-esp32-qemu` sibling; the QEMU board reuses the same
//! ESP-HAL timer surfaces, so the hook contract is identical. See
//! the esp32 module for usage docs.

use core::sync::atomic::{AtomicPtr, Ordering};
use nros_platform_api::TimerError;

pub type RegisterPeriodicFn = extern "C" fn(
    period_us: u32,
    callback: extern "C" fn(*mut core::ffi::c_void),
    user_data: *mut core::ffi::c_void,
) -> i32;
pub type DestroyPeriodicFn = extern "C" fn();

static REGISTER_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
static DESTROY_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

pub fn install_periodic_timer_hook(register: RegisterPeriodicFn, destroy: DestroyPeriodicFn) {
    REGISTER_FN.store(register as *mut (), Ordering::Release);
    DESTROY_FN.store(destroy as *mut (), Ordering::Release);
}

pub(crate) fn dispatch_register(
    period_us: u32,
    callback: extern "C" fn(*mut core::ffi::c_void),
    user_data: *mut core::ffi::c_void,
) -> Result<(), TimerError> {
    let raw = REGISTER_FN.load(Ordering::Acquire);
    if raw.is_null() {
        return Err(TimerError::Unsupported);
    }
    let f: RegisterPeriodicFn = unsafe { core::mem::transmute(raw) };
    let rc = f(period_us, callback, user_data);
    match rc {
        0 => Ok(()),
        -2 => Err(TimerError::OutOfRange),
        _ => Err(TimerError::KernelError),
    }
}

pub(crate) fn dispatch_destroy() {
    let raw = DESTROY_FN.load(Ordering::Acquire);
    if raw.is_null() {
        return;
    }
    let f: DestroyPeriodicFn = unsafe { core::mem::transmute(raw) };
    f();
}
