//! XRCE-DDS platform symbols for QEMU MPS2-AN385.
//!
//! Provides the FFI symbols needed by XRCE-DDS on bare-metal:
//! - `uxr_millis()` / `uxr_nanos()` — called from `session.c`, `ping.c`
//!   (since `time.c` is skipped for bare-metal builds)
//! - `smoltcp_clock_now_ms()` — called from `xrce-smoltcp` transport
//!
//! Uses the CMSDK APB Timer0 as a free-running hardware clock source.
//! The timer runs at 25 MHz and wraps every ~171 seconds; wrap detection
//! extends the range to ~49 days via an atomic wrap counter.
//!
//! # Comparison with zpico-platform-mps2-an385
//!
//! | zpico (zenoh-pico) | xrce (XRCE-DDS) |
//! |----|-----|
//! | ~55 symbols (memory, clock, RNG, sleep, threading, sockets, libc) | 3 symbols (uxr_millis, uxr_nanos, smoltcp_clock_now_ms) |

#![no_std]

use core::sync::atomic::{AtomicU32, Ordering};

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
/// missing a wrap. In practice, XRCE-DDS session management calls
/// `uxr_millis()` far more frequently than this.
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

// ============================================================================
// FFI Exports — XRCE-DDS time symbols
// ============================================================================

/// `int64_t uxr_millis(void)` — current time in milliseconds.
///
/// Called by `session.c` and `ping.c` for timeout management.
/// Replaces `time.c` which is only compiled for POSIX builds.
#[unsafe(no_mangle)]
pub extern "C" fn uxr_millis() -> i64 {
    clock_ms() as i64
}

/// `int64_t uxr_nanos(void)` — current time in nanoseconds.
///
/// Called by `session.c` for time synchronization.
/// Replaces `time.c` which is only compiled for POSIX builds.
#[unsafe(no_mangle)]
pub extern "C" fn uxr_nanos() -> i64 {
    (clock_ms() as i64).wrapping_mul(1_000_000)
}

// ============================================================================
// FFI Export — smoltcp transport clock
// ============================================================================

/// Get current time in milliseconds (called by `xrce-smoltcp`).
///
/// Same symbol name as zpico-smoltcp uses, so platform crates only
/// need one clock source for either transport backend.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}
