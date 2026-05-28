//! Phase 110.E.b — periodic-callback hook for STM32F4 sporadic-server
//! budget refill.
//!
//! STM32F4 has multiple general-purpose timers (TIM2/3/4/5 32-bit on
//! some variants, 16-bit elsewhere). Picking + initialising a specific
//! one is a per-board decision: the board crate owns the
//! `stm32f4xx_hal::pac::Peripherals` handle and arranges clock-tree
//! configuration. This module therefore exposes a *hook surface*
//! rather than driving a fixed peripheral directly.
//!
//! ## Usage
//!
//! A board crate (or user-init code) calls
//! [`install_periodic_timer_hook`] once during boot with function
//! pointers that know how to register / destroy a periodic IRQ on
//! whichever timer the board has reserved. The platform's
//! [`PlatformTimer::create_periodic`] impl dispatches through these
//! hooks. With no hook installed, `create_periodic` returns
//! [`TimerError::Unsupported`] so cross-platform code degrades
//! gracefully.
//!
//! ## Example (board crate)
//!
//! ```ignore
//! use stm32f4xx_hal::{prelude::*, timer::Timer};
//!
//! // In `init_hardware`:
//! let tim2_cb_slot = setup_tim2_periodic_irq();
//! nros_platform_stm32f4::sporadic_timer::install_periodic_timer_hook(
//!     tim2_cb_slot.register,
//!     tim2_cb_slot.destroy,
//! );
//! ```
//!
//! The mps2-an385 sibling module ships a complete CMSDK-Timer1
//! reference implementation; see
//! `packages/platforms/nros-platform-mps2-an385/src/sporadic_timer.rs`
//! for the canonical pattern.

use core::sync::atomic::{AtomicPtr, Ordering};
use nros_platform_api::TimerError;

/// Hook signature for "register a periodic callback firing every
/// `period_us` µs". Returns `Ok(())` on success, [`TimerError`] on
/// failure (e.g. out-of-range period for the hardware timer's
/// counter width).
pub type RegisterPeriodicFn = extern "C" fn(
    period_us: u32,
    callback: extern "C" fn(*mut core::ffi::c_void),
    user_data: *mut core::ffi::c_void,
) -> i32;

/// Hook signature for "stop the periodic callback". Idempotent.
pub type DestroyPeriodicFn = extern "C" fn();

static REGISTER_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
static DESTROY_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Install the per-board periodic-timer hooks. Call once during
/// boot from the board crate's `init_hardware` (or equivalent).
/// Calling again replaces the previous hooks.
pub fn install_periodic_timer_hook(register: RegisterPeriodicFn, destroy: DestroyPeriodicFn) {
    REGISTER_FN.store(register as *mut (), Ordering::Release);
    DESTROY_FN.store(destroy as *mut (), Ordering::Release);
}

/// Dispatch `create_periodic` through the installed hook. Returns
/// `Ok(())` if a hook is installed and accepted the registration,
/// `Err(TimerError::Unsupported)` if no hook is installed.
pub(crate) fn dispatch_register(
    period_us: u32,
    callback: extern "C" fn(*mut core::ffi::c_void),
    user_data: *mut core::ffi::c_void,
) -> Result<(), TimerError> {
    let raw = REGISTER_FN.load(Ordering::Acquire);
    if raw.is_null() {
        return Err(TimerError::Unsupported);
    }
    // SAFETY: only `install_periodic_timer_hook` writes this slot,
    // and it always stores a real `RegisterPeriodicFn` pointer.
    let f: RegisterPeriodicFn = unsafe { core::mem::transmute(raw) };
    let rc = f(period_us, callback, user_data);
    match rc {
        0 => Ok(()),
        -2 => Err(TimerError::OutOfRange),
        _ => Err(TimerError::KernelError),
    }
}

/// Dispatch `destroy` through the installed hook. Idempotent; no-op
/// when no hook is installed.
pub(crate) fn dispatch_destroy() {
    let raw = DESTROY_FN.load(Ordering::Acquire);
    if raw.is_null() {
        return;
    }
    let f: DestroyPeriodicFn = unsafe { core::mem::transmute(raw) };
    f();
}
