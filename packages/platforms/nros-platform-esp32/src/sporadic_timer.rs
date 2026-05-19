//! Phase 110.E.b — periodic-callback hook for ESP32-C3 sporadic-server
//! budget refill.
//!
//! ESP32-C3 has multiple timer sources usable for periodic IRQs
//! (`SystemTimer`, `TIMG0/1`'s 64-bit alarms). esp-hal owns these
//! peripherals at board init time and hands them out via the
//! `Peripherals` split. This module therefore exposes a hook surface
//! rather than driving a fixed peripheral directly — the board crate
//! (or user-init) calls
//! [`install_periodic_timer_hook`] once with function pointers that
//! know how to register / destroy a periodic IRQ on whichever timer
//! the board has reserved.
//!
//! See `nros-platform-mps2-an385::sporadic_timer` for the canonical
//! "drive the timer directly" reference; ESP-HAL's ownership model
//! makes that pattern impractical inside the platform crate itself.

use core::sync::atomic::{AtomicPtr, Ordering};
use nros_platform_api::TimerError;

/// Hook signature for "register a periodic callback firing every
/// `period_us` µs". Returns `0` on success, `-2` for
/// [`TimerError::OutOfRange`], anything else for
/// [`TimerError::KernelError`].
pub type RegisterPeriodicFn =
    extern "C" fn(period_us: u32, callback: extern "C" fn(*mut core::ffi::c_void), user_data: *mut core::ffi::c_void) -> i32;

/// Hook signature for "stop the periodic callback". Idempotent.
pub type DestroyPeriodicFn = extern "C" fn();

static REGISTER_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
static DESTROY_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Install the per-board periodic-timer hooks. Call once during
/// boot from the board crate's `init_hardware` (or equivalent).
/// Calling again replaces the previous hooks.
pub fn install_periodic_timer_hook(
    register: RegisterPeriodicFn,
    destroy: DestroyPeriodicFn,
) {
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
