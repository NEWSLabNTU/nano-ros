//! Hardware-backed monotonic clock for bare-metal MPS2-AN385.
//!
//! Uses the CMSDK APB Timer0 as a free-running hardware clock source.
//! The timer runs at 25 MHz (SYSCLK) and wraps every ~171 seconds;
//! wrap detection extends the range to ~49 days via an atomic wrap counter.

use core::sync::atomic::{AtomicU32, Ordering};

// ============================================================================
// CMSDK APB Timer0 Registers (base 0x4000_0000)
// ============================================================================

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
/// missing a wrap. In practice, zenoh-pico and XRCE-DDS call their clock
/// functions far more frequently than this.
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
    let elapsed_ticks = wraps * 0x1_0000_0000u64 + (0xFFFF_FFFFu64 - current as u64);
    elapsed_ticks / TICKS_PER_MS
}
