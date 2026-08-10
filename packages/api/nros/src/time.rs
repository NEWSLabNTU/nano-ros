//! Monotonic time for portable node code (issue #504).
//!
//! Node packages are platform-agnostic: the same crate compiles into a
//! POSIX, FreeRTOS, or Zephyr image, so it can reach neither the
//! per-platform timing types nor the `nros_platform_clock_us` C export
//! directly. This module is the portable spelling. Uses that motivated
//! it: `dt` in control laws (assuming the nominal period silently
//! misintegrates whenever a callback runs late), timestamping published
//! state, and coarse in-node watchdogs.
//!
//! Semantics: monotonic since an unspecified epoch (process start or
//! boot). This is explicitly NOT ROS time — no sim-time, no epoch
//! meaning, no cross-machine comparability. Compare instants, never
//! interpret one absolutely.
//!
//! Clock source mirrors the executor's timer accounting
//! (`nros-node/src/executor/spin.rs`):
//!
//! - `std` builds: [`std::time::Instant`], anchored at first use.
//! - `no_std` + `rmw-cffi` builds: the platform's
//!   `nros_platform_clock_us` export — the same linkage contract the
//!   executor and the wake primitives already rely on, so this adds no
//!   new requirement. Resolution is whatever the platform delivers
//!   (issue #502: sub-tick on FreeRTOS Cortex-M, tick-quantized on
//!   ThreadX).
//!
//! A `no_std` build without `rmw-cffi` has no clock source; this
//! module is absent there.

#![allow(clippy::module_name_repetitions)]

use core::time::Duration;

/// Monotonic time since an unspecified epoch.
///
/// ```ignore
/// let t0 = nros::time::now();
/// // ... work ...
/// let dt = nros::time::now() - t0;
/// ```
#[cfg(feature = "std")]
#[must_use]
pub fn now() -> Duration {
    use std::{sync::OnceLock, time::Instant};
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed()
}

/// Monotonic time since an unspecified epoch.
///
/// Reads the platform port's `nros_platform_clock_us` (microsecond
/// resolution at best — see issue #502 for per-port truth).
#[cfg(all(not(feature = "std"), feature = "rmw-cffi"))]
#[must_use]
pub fn now() -> Duration {
    unsafe extern "C" {
        fn nros_platform_clock_us() -> u64;
    }
    // SAFETY: bare query of the platform's monotonic us counter; the
    // symbol is guaranteed by whichever platform port linked the
    // binary (the same contract the executor's default timer clock
    // depends on).
    Duration::from_micros(unsafe { nros_platform_clock_us() })
}

/// Convenience: [`now`] as whole microseconds.
#[cfg(any(feature = "std", feature = "rmw-cffi"))]
#[must_use]
pub fn now_us() -> u64 {
    u64::try_from(now().as_micros()).unwrap_or(u64::MAX)
}
