//! Platform abstraction for nros C API.
//!
//! This module provides platform-specific functionality via FFI to C platform
//! implementations. For `std` builds, Rust implementations are used directly.
//! For `no_std` builds, the C platform layer is called.
//!
//! The C platform layer is selected at compile time via preprocessor macros:
//! - `NROS_PLATFORM_POSIX` - Linux, macOS, POSIX systems
//! - `NROS_PLATFORM_ZEPHYR` - Zephyr RTOS
//! - `NROS_PLATFORM_FREERTOS` - FreeRTOS
//! - `NROS_PLATFORM_BAREMETAL` - Bare-metal (user provides time/sleep)
//! - `NROS_PLATFORM_CUSTOM` - User provides all functions

// ============================================================================
// FFI Declarations (for no_std)
// ============================================================================

// phase-243 — the no_std time/sleep path is built on the canonical platform ABI's
// monotonic µs clock (`nros-platform-api`), not the retired A-only ns symbols.
// Atomics no longer cross the FFI at all — they use `core::sync::atomic` on both
// std and no_std (see below).
// cbindgen:ignore
//
// phase-359 W10 — no longer `no_std`-only. Every consumer of this crate is a C
// image, and a C image links a platform port; asking the port is what the rest
// of the tree does on both flavours since this campaign unified `nros-node`'s
// clock. The `std` twins these replaced are described at each function.
unsafe extern "C" {
    /// Monotonic nanoseconds since a platform-defined epoch (RFC-0073).
    fn nros_platform_clock_ns() -> u64;

    /// Sleep at least `us` microseconds.
    fn nros_platform_sleep_us(us: usize);

    /// Nanoseconds since the Unix epoch (issue 0532).
    ///
    /// ONE symbol, replacing the `time_since_epoch_{secs,nanos}` pair. Those are
    /// RETIRED — nothing defines them any more, so a reference here is not a
    /// deprecation warning but an undefined symbol at link time.
    fn nros_platform_time_now_ns() -> u64;
}

// ============================================================================
// Time Functions
// ============================================================================

/// Get current monotonic time in nanoseconds.
///
/// phase-359 W10 — one implementation. The `std` twin kept its own epoch in a
/// `OnceLock<Instant>` while the platform already had one, so the two flavours
/// answered "how long have we been running" from different clocks.
///
/// RFC-0073 made nanoseconds the ABI's own unit, so this stopped fabricating
/// them by multiplying microseconds. What the low digits are worth is a
/// question `nros_platform_clock_resolution_ns` answers.
pub fn get_time_ns() -> u64 {
    // SAFETY: a bare counter read, no pointer arguments, guaranteed by whichever
    // port linked the image.
    unsafe { nros_platform_clock_ns() }
}

/// Get system time in nanoseconds since the Unix epoch.
///
/// phase-359 W10 — this FIXES the no_std answer rather than merely merging two.
/// The `no_std` twin returned `get_time_ns()`, the MONOTONIC counter,
/// documented as "returns monotonic time as system time is not available". It
/// IS available: the ABI has `nros_platform_time_since_epoch_*` and every port
/// implements it. A caller stamping a message with this on target was getting
/// time-since-boot presented as time-since-1970.
pub fn get_system_time_ns() -> i64 {
    // issue 0532 — this is the "remaining half" the previous comment here
    // promised, and it is a DELETION rather than a rewrite.
    //
    // The ABI used to spend one instant over TWO symbols, each sampling the
    // clock separately, so a second boundary between the reads paired the old
    // whole second with the new sub-second remainder — a stamp that jumps a
    // second backwards. This function carried a bounded re-read loop to narrow
    // that window, which could not close it: any loop over two clocks is still
    // two clocks.
    //
    // `nros_platform_time_now_ns` is one read of one clock, so the torn stamp is
    // not narrowed but impossible, and the loop goes with the symbols. The pair
    // is retired, not deprecated — leaving the old call here was an undefined
    // symbol at link time, which is how it was found.
    //
    // SAFETY: a bare wall-clock read, no pointer arguments.
    unsafe { nros_platform_time_now_ns() as i64 }
}

// ============================================================================
// Sleep Functions
// ============================================================================

/// Sleep for the specified duration in nanoseconds.
///
/// phase-359 W10 — one implementation, the platform's own pacing primitive.
/// Rounded to microseconds because that is the ABI's unit; the `std` twin took
/// nanoseconds and handed them to `thread::sleep`, whose sub-microsecond
/// precision is nominal on every OS this runs on.
pub fn sleep_ns(ns: u64) {
    // SAFETY: a bare pacing call with no pointer arguments.
    unsafe { nros_platform_sleep_us((ns / 1000) as usize) }
}

// ============================================================================
// Atomic Operations
// ============================================================================

/// Atomically store a boolean value with release semantics.
///
/// phase-243: `core::sync::atomic` on BOTH std and no_std (was an FFI call to the
/// A-only `nros_platform_atomic_store_bool` on no_std). A naturally-aligned
/// `AtomicBool` store is lock-free on every target nros builds.
///
/// # Safety
/// `ptr` must be valid + properly aligned for the access.
pub fn atomic_store_bool(ptr: *mut bool, value: bool) {
    use core::sync::atomic::{AtomicBool, Ordering};
    unsafe {
        let atomic_ptr = ptr as *const AtomicBool;
        (*atomic_ptr).store(value, Ordering::Release);
    }
}

/// Atomically load a boolean value with acquire semantics.
///
/// phase-243: `core::sync::atomic` on BOTH std and no_std.
///
/// # Safety
/// `ptr` must be valid + properly aligned for the access.
pub fn atomic_load_bool(ptr: *const bool) -> bool {
    use core::sync::atomic::{AtomicBool, Ordering};
    unsafe {
        let atomic_ptr = ptr as *const AtomicBool;
        (*atomic_ptr).load(Ordering::Acquire)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_time_ns() {
        let t1 = get_time_ns();
        let t2 = get_time_ns();
        // Time should be monotonically increasing (or at least not decreasing)
        assert!(t2 >= t1);
    }

    #[test]
    fn test_get_system_time_ns() {
        let t = get_system_time_ns();
        // Should be a positive value (after Unix epoch)
        assert!(t > 0);
    }

    #[test]
    fn test_sleep_ns() {
        let start = get_time_ns();
        sleep_ns(1_000_000); // 1ms
        let elapsed = get_time_ns() - start;
        // Should have slept at least 500us (allowing for timing imprecision)
        assert!(elapsed >= 500_000);
    }

    #[test]
    fn test_atomic_bool() {
        let mut value = false;
        atomic_store_bool(&mut value, true);
        assert!(atomic_load_bool(&value));
        atomic_store_bool(&mut value, false);
        assert!(!atomic_load_bool(&value));
    }
}
