//! Phase 110.E.b — CMSDK Timer1 periodic ISR for sporadic-server
//! budget refill on MPS2-AN385.
//!
//! Provides ONE periodic-callback slot. Backs the
//! `PlatformTimer::create_periodic` impl on `Mps2An385Platform`. The
//! `Executor::register_sporadic_timer` path arranges for
//! `atomic_sporadic_refill_thunk(&AtomicSporadicState)` to fire each
//! tick; the handler performs a single atomic store + returns.
//!
//! ## Hardware
//!
//! CMSDK Timer1 (base `0x4000_1000`) — same APB Timer block as the
//! Timer0 free-running clock in `clock.rs`, just the second
//! instance. Runs at 25 MHz (SYSCLK). 32-bit reload counter →
//! periods from ~40 ns up to ~171 s representable directly without
//! a software accumulator.
//!
//! ## NVIC
//!
//! `mps2_an385_pac::Interrupt::TIMER1` (NVIC index 9). The
//! `#[interrupt] fn TIMER1()` symbol below is the cortex-m-rt
//! exception entry; the linker resolves the vector table slot at
//! `mps2_an385_pac::__INTERRUPTS[9]`.
//!
//! ## Scope
//!
//! v1 supports a single registered periodic callback. The trait's
//! `create_oneshot` returns `Unsupported` for now — the per-callback
//! overrun-detection oneshot needs its own slot + an unrelated
//! Timer / DualTimer, deferred per design.

use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use cortex_m::peripheral::NVIC;
use nros_platform_api::TimerError;

/// CMSDK Timer1 base address.
const TIMER1_BASE: usize = 0x4000_1000;
const TIMER_CTRL: usize = 0x00;
/// Interrupt status / clear register offset (write 1 to clear).
const TIMER_INTSTATUS: usize = 0x0C;
const TIMER_RELOAD: usize = 0x08;

/// CTRL register bits.
const CTRL_EN: u32 = 1 << 0;
const CTRL_IRQEN: u32 = 1 << 3;

/// System clock frequency (Hz).
const SYSCLK_HZ: u64 = 25_000_000;

/// `cortex_m` NVIC peripheral number for the CMSDK Timer1 interrupt.
const TIMER1_IRQ_NUM: u8 = 9;

/// Registered callback (function pointer). `null` = no callback
/// installed; the ISR exits cleanly. `AtomicPtr` so the ISR sees
/// a coherent value without a critical-section read.
static CALLBACK_FN: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Registered callback user data.
static CALLBACK_CTX: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());

/// Tracks whether the timer hardware has been initialised so
/// `destroy` can stop it idempotently.
static ENABLED: AtomicU32 = AtomicU32::new(0);

/// Install a periodic callback firing every `period_us` microseconds.
///
/// v1 supports one global slot — calling again replaces the previous
/// callback and reprograms the period. Returns `Unsupported` if
/// `period_us == 0`; `OutOfRange` if the converted tick count would
/// exceed the 32-bit reload register.
pub fn register_periodic(
    period_us: u32,
    callback: extern "C" fn(*mut core::ffi::c_void),
    user_data: *mut core::ffi::c_void,
) -> Result<(), TimerError> {
    if period_us == 0 {
        return Err(TimerError::Unsupported);
    }
    let ticks_u64 = (period_us as u64) * SYSCLK_HZ / 1_000_000;
    if ticks_u64 == 0 || ticks_u64 > u32::MAX as u64 {
        return Err(TimerError::OutOfRange);
    }
    // Store the callback BEFORE arming the timer so the first IRQ
    // sees a coherent slot. Function-pointer cast through `*mut ()`
    // matches `AtomicPtr`'s storage shape; the ISR casts back to
    // the original signature.
    CALLBACK_CTX.store(user_data, Ordering::Release);
    CALLBACK_FN.store(callback as *mut (), Ordering::Release);
    unsafe {
        let base = TIMER1_BASE as *mut u32;
        // Disable first so reload + intstatus writes apply cleanly.
        core::ptr::write_volatile(base.byte_add(TIMER_CTRL), 0);
        core::ptr::write_volatile(base.byte_add(TIMER_RELOAD), ticks_u64 as u32);
        // Clear any pending interrupt latched while disabled.
        core::ptr::write_volatile(base.byte_add(TIMER_INTSTATUS), 1);
        // Enable counter + IRQ.
        core::ptr::write_volatile(base.byte_add(TIMER_CTRL), CTRL_EN | CTRL_IRQEN);
    }
    // Unmask the NVIC slot last so the IRQ can fire only after the
    // callback slot is fully populated.
    unsafe { NVIC::unmask(mps2_an385_pac::Interrupt::TIMER1) };
    ENABLED.store(1, Ordering::Release);
    Ok(())
}

/// Stop the timer + clear the callback slot. Idempotent.
pub fn destroy() {
    if ENABLED.swap(0, Ordering::AcqRel) == 0 {
        return;
    }
    NVIC::mask(mps2_an385_pac::Interrupt::TIMER1);
    unsafe {
        let base = TIMER1_BASE as *mut u32;
        core::ptr::write_volatile(base.byte_add(TIMER_CTRL), 0);
        core::ptr::write_volatile(base.byte_add(TIMER_INTSTATUS), 1);
    }
    CALLBACK_FN.store(core::ptr::null_mut(), Ordering::Release);
    CALLBACK_CTX.store(core::ptr::null_mut(), Ordering::Release);
    let _ = TIMER1_IRQ_NUM;
}

/// Exception handler for CMSDK Timer1. Reads the registered
/// callback under Acquire ordering, clears the hardware IRQ latch,
/// and invokes the callback with `user_data`. Body must stay short
/// — the design doc's lowest-common-denominator contract is "atomic
/// ops only, no heap, no blocking primitives".
///
/// Defined as a `pub extern "C" fn` so the `#[unsafe(no_mangle)]`
/// alias below resolves the vector-table slot without cortex-m-rt
/// in the dependency closure.
#[unsafe(no_mangle)]
pub extern "C" fn TIMER1() {
    unsafe {
        let base = TIMER1_BASE as *mut u32;
        core::ptr::write_volatile(base.byte_add(TIMER_INTSTATUS), 1);
    }
    let cb_raw = CALLBACK_FN.load(Ordering::Acquire);
    if cb_raw.is_null() {
        return;
    }
    let ctx = CALLBACK_CTX.load(Ordering::Acquire);
    // SAFETY: `cb_raw` was stored from a
    // `extern "C" fn(*mut c_void)` cast via `*mut ()` above. The
    // round-trip is sound when the writer-side store is the only
    // mutation path (true: `register_periodic` is the sole writer,
    // and it stores ctx before fn so the IRQ never sees a fn with
    // stale ctx).
    let cb: extern "C" fn(*mut core::ffi::c_void) = unsafe { core::mem::transmute(cb_raw) };
    cb(ctx);
}
