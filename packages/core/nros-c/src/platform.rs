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

/// cbindgen:ignore
#[cfg(not(feature = "std"))]
unsafe extern "C" {
    /// Get current monotonic time in nanoseconds.
    pub fn nros_platform_time_ns() -> u64;

    /// Sleep for the specified duration in nanoseconds.
    pub fn nros_platform_sleep_ns(ns: u64);

    /// Atomically store a boolean value with release semantics.
    pub fn nros_platform_atomic_store_bool(ptr: *mut bool, value: bool);

    /// Atomically load a boolean value with acquire semantics.
    pub fn nros_platform_atomic_load_bool(ptr: *const bool) -> bool;
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
#[cfg(not(feature = "std"))]
pub fn get_time_ns() -> u64 {
    unsafe { nros_platform_time_ns() }
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
#[cfg(not(feature = "std"))]
pub fn sleep_ns(ns: u64) {
    unsafe { nros_platform_sleep_ns(ns) }
}

// ============================================================================
// Atomic Operations
// ============================================================================

/// Atomically store a boolean value with release semantics.
///
/// For `std` builds, uses `std::sync::atomic`.
/// For `no_std` builds, calls the C platform function.
#[cfg(feature = "std")]
pub fn atomic_store_bool(ptr: *mut bool, value: bool) {
    use core::sync::atomic::{AtomicBool, Ordering};
    // Safety: caller must ensure ptr is valid
    unsafe {
        let atomic_ptr = ptr as *const AtomicBool;
        (*atomic_ptr).store(value, Ordering::Release);
    }
}

/// Atomically store a boolean value (no_std version).
#[cfg(not(feature = "std"))]
pub fn atomic_store_bool(ptr: *mut bool, value: bool) {
    unsafe { nros_platform_atomic_store_bool(ptr, value) }
}

/// Atomically load a boolean value with acquire semantics.
///
/// For `std` builds, uses `std::sync::atomic`.
/// For `no_std` builds, calls the C platform function.
#[cfg(feature = "std")]
pub fn atomic_load_bool(ptr: *const bool) -> bool {
    use core::sync::atomic::{AtomicBool, Ordering};
    // Safety: caller must ensure ptr is valid
    unsafe {
        let atomic_ptr = ptr as *const AtomicBool;
        (*atomic_ptr).load(Ordering::Acquire)
    }
}

/// Atomically load a boolean value (no_std version).
#[cfg(not(feature = "std"))]
pub fn atomic_load_bool(ptr: *const bool) -> bool {
    unsafe { nros_platform_atomic_load_bool(ptr) }
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
