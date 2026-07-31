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
#[cfg(not(feature = "std"))]
unsafe extern "C" {
    /// Monotonic microseconds since a platform-defined epoch.
    fn nros_platform_clock_us() -> u64;

    /// Sleep at least `us` microseconds.
    fn nros_platform_sleep_us(us: usize);
}

// ============================================================================
// Time Functions
// ============================================================================

/// Get current monotonic time in nanoseconds.
///
/// For `std` builds, uses `std::time::Instant`.
/// For `no_std` builds, calls the C platform function.
#[cfg(feature = "std")]
pub fn get_time_ns() -> u64 {
    use std::time::Instant;

    // Use a static reference point for monotonic time
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    Instant::now().duration_since(*epoch).as_nanos() as u64
}

/// Get current monotonic time in nanoseconds (no_std version).
///
/// phase-243: derived from the canonical platform µs clock (`clock_us * 1000`).
/// The callers are ms-scale spins/deadlines (executor/service/action), so µs
/// granularity is ample; add a dedicated `clock_ns` to the ABI if ns is ever
/// needed rather than resurrecting the A ns-clock.
#[cfg(not(feature = "std"))]
pub fn get_time_ns() -> u64 {
    unsafe { nros_platform_clock_us().wrapping_mul(1000) }
}

/// Get system time in nanoseconds since Unix epoch.
///
/// For `std` builds, uses `std::time::SystemTime`.
/// For `no_std` builds, returns monotonic time (no Unix epoch available).
#[cfg(feature = "std")]
pub fn get_system_time_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as i64,
        Err(_) => 0,
    }
}

/// Get system time in nanoseconds (no_std version).
///
/// Note: For no_std, this returns monotonic time as system time is not available.
#[cfg(not(feature = "std"))]
pub fn get_system_time_ns() -> i64 {
    get_time_ns() as i64
}

// ============================================================================
// Sleep Functions
// ============================================================================

/// Sleep for the specified duration in nanoseconds.
///
/// For `std` builds, uses `std::thread::sleep`.
/// For `no_std` builds, calls the C platform function.
#[cfg(feature = "std")]
pub fn sleep_ns(ns: u64) {
    use std::time::Duration;
    std::thread::sleep(Duration::from_nanos(ns));
}

/// Sleep for the specified duration in nanoseconds (no_std version).
///
/// phase-243: forwarded to the canonical platform µs sleep (`sleep_us(ns/1000)`).
#[cfg(not(feature = "std"))]
pub fn sleep_ns(ns: u64) {
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
