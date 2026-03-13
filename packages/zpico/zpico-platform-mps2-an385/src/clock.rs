//! Hardware-backed monotonic clock for bare-metal MPS2-AN385
//!
//! Uses the CMSDK APB Timer0 as a free-running hardware clock source.
//! The timer runs at 25 MHz (SYSCLK) and wraps every ~171 seconds;
//! wrap detection extends the range to ~49 days via an atomic wrap counter.
//!
//! # QEMU clock synchronization
//!
//! QEMU's virtual clock and wall-clock diverge by default: WFI advances
//! virtual time instantly while TAP network I/O requires wall-clock time.
//! This is solved at the QEMU launch level with `-icount shift=auto`,
//! which makes virtual time track wall-clock time during WFI (the `sleep=on`
//! default uses `QEMU_CLOCK_VIRTUAL_RT` to advance gradually). See
//! `docs/reference/qemu-icount.md` for details.
//!
//! On **real hardware**, the hardware timer naturally tracks wall-clock time,
//! so no special handling is needed.
//!
//! Implements zenoh-pico clock symbols directly (`z_clock_now`, `z_clock_elapsed_*`,
//! `z_clock_advance_*`) plus `smoltcp_clock_now_ms` for the transport crate.
//!
//! Note: Uses AtomicU32 for Cortex-M3 compatibility (no native 64-bit atomics).

use core::sync::atomic::{AtomicU32, Ordering};
#[cfg(feature = "ethernet")]
use smoltcp::time::Instant;

// ============================================================================
// CMSDK APB Timer0 Registers (base 0x4000_0000)
// ============================================================================
//
// The CMSDK APB Timer is a 32-bit down-counter. We configure it as a
// free-running counter by loading RELOAD with 0xFFFF_FFFF and enabling it.
// Reading VALUE gives the current count (decreasing). The timer runs at the
// system clock frequency (25 MHz on MPS2-AN385).

/// CMSDK Timer0 base address
const TIMER0_BASE: usize = 0x4000_0000;
/// Control register offset
const TIMER_CTRL: usize = 0x00;
/// Current value register offset
const TIMER_VALUE: usize = 0x04;
/// Reload value register offset
const TIMER_RELOAD: usize = 0x08;

/// Control register bit: enable the timer
const CTRL_EN: u32 = 1 << 0;

/// System clock frequency (MPS2-AN385 runs at 25 MHz)
const SYSCLK_HZ: u64 = 25_000_000;

/// Ticks per millisecond
const TICKS_PER_MS: u64 = SYSCLK_HZ / 1000;

// ============================================================================
// Hardware Timer Access
// ============================================================================

/// Last VALUE register reading (for wrap detection).
/// The timer counts down, so a value larger than the last reading means a wrap.
static LAST_VALUE: AtomicU32 = AtomicU32::new(0);

/// Number of times the 32-bit counter has wrapped.
static WRAP_COUNT: AtomicU32 = AtomicU32::new(0);

/// Initialize CMSDK Timer0 as a free-running counter.
///
/// Must be called before any clock reads. Safe to call multiple times
/// (idempotent).
pub fn init_hardware_timer() {
    unsafe {
        let base = TIMER0_BASE as *mut u32;
        // Load max reload value for free-running mode
        core::ptr::write_volatile(base.byte_add(TIMER_RELOAD), 0xFFFF_FFFF);
        // Enable the timer (no interrupt, no external input)
        core::ptr::write_volatile(base.byte_add(TIMER_CTRL), CTRL_EN);
    }
    // Seed LAST_VALUE with the current reading
    LAST_VALUE.store(read_timer_value(), Ordering::Relaxed);
}

/// Read the raw timer VALUE register.
#[inline]
fn read_timer_value() -> u32 {
    unsafe {
        let base = TIMER0_BASE as *const u32;
        core::ptr::read_volatile(base.byte_add(TIMER_VALUE))
    }
}

/// Get the current time in milliseconds from the hardware timer.
///
/// Handles 32-bit wrap-around by tracking the wrap count. The timer counts
/// down from 0xFFFF_FFFF at 25 MHz, wrapping every ~171.8 seconds.
/// With a 32-bit wrap counter, this extends to ~49 days.
///
/// This function must be called at least once per ~171 seconds to avoid
/// missing a wrap. In practice, zenoh-pico calls `z_clock_elapsed_ms()`
/// far more frequently than this.
pub fn clock_ms() -> u64 {
    let current = read_timer_value();
    let last = LAST_VALUE.load(Ordering::Relaxed);

    // The timer counts DOWN. If current > last, the counter wrapped
    // past zero and reloaded from 0xFFFF_FFFF.
    if current > last {
        WRAP_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    LAST_VALUE.store(current, Ordering::Relaxed);

    let wraps = WRAP_COUNT.load(Ordering::Relaxed) as u64;
    // Elapsed ticks = (wraps * 2^32) + (0xFFFF_FFFF - current)
    // The timer counts down, so 0xFFFF_FFFF - current = elapsed ticks
    // since the last reload.
    let elapsed_ticks = wraps * 0x1_0000_0000u64 + (0xFFFF_FFFFu64 - current as u64);
    elapsed_ticks / TICKS_PER_MS
}

/// Get the current time as a smoltcp Instant
#[cfg(feature = "ethernet")]
#[inline]
pub fn now() -> Instant {
    Instant::from_millis(clock_ms() as i64)
}

// ============================================================================
// FFI exports — zenoh-pico clock symbols
// ============================================================================
//
// z_clock_t is `void*` on bare-metal (zenoh-pico's void.h), so it is
// pointer-sized: 4 bytes on ARM32, 8 bytes on 64-bit targets. All clock
// functions must use `usize` (not `u64`) for the stored timestamp type
// to match the C ABI. The lower 32 bits of clock_ms() are sufficient
// (~49 days of uptime).

/// z_clock_t z_clock_now(void)
#[unsafe(no_mangle)]
pub extern "C" fn z_clock_now() -> usize {
    clock_ms() as usize
}

/// unsigned long z_clock_elapsed_us(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_us(time: *const usize) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    let now = clock_ms() as usize;
    let elapsed_ms = now.wrapping_sub(start);
    (elapsed_ms * 1000) as core::ffi::c_ulong
}

/// unsigned long z_clock_elapsed_ms(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_ms(time: *const usize) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    let now = clock_ms() as usize;
    now.wrapping_sub(start) as core::ffi::c_ulong
}

/// unsigned long z_clock_elapsed_s(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_s(time: *const usize) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    let now = clock_ms() as usize;
    let elapsed_ms = now.wrapping_sub(start);
    (elapsed_ms / 1000) as core::ffi::c_ulong
}

/// void z_clock_advance_us(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_us(clock: *mut usize, duration: core::ffi::c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add((duration as usize).div_ceil(1000));
    }
}

/// void z_clock_advance_ms(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_ms(clock: *mut usize, duration: core::ffi::c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add(duration as usize);
    }
}

/// void z_clock_advance_s(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_s(clock: *mut usize, duration: core::ffi::c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add(duration as usize * 1000);
    }
}

// ============================================================================
// FFI export — transport crate needs this for smoltcp timestamping
// ============================================================================

/// Get current time in milliseconds (called by zpico-smoltcp's bridge)
#[cfg(feature = "ethernet")]
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}
