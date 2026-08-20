//! Monotonic time for portable node code (issue #504).
//!
//! Node packages are platform-agnostic: the same crate compiles into a
//! POSIX, FreeRTOS, or Zephyr image, so it can reach neither the
//! per-platform timing types nor the `nros_platform_clock_ns` C export
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
//! (`nros-node/src/executor/spin.rs`), and phase-359 W10 follow-up had to
//! RESTORE that — the sentence had gone false:
//!
//! - **`rmw-cffi` builds** (a platform port is linked): the platform's
//!   `nros_platform_clock_ns` export, on either flavour. Same linkage contract
//!   the executor and the wake primitives already rely on, so this adds no new
//!   requirement. Resolution is whatever the platform delivers (issue #502:
//!   sub-tick on FreeRTOS Cortex-M, tick-quantized on ThreadX).
//! - **`std` without a port**: [`std::time::Instant`], anchored at first use.
//!   The claim that stood here — "real and shipped, the metadata probe compiles
//!   node code with `std` and no port" — was WRONG. The probe links
//!   `nros-platform-cffi` with `posix-c-port` and resolves `rmw-cffi`, so it
//!   takes the arm above. This arm has no in-tree consumer: nothing calls
//!   `now()` anywhere in the tree.
//! - **Neither**: no clock source; this module is absent.
//!
//! The order used to be the other way round, `std` first, which meant every
//! NATIVE build (std AND a port) read `Instant` here while the executor read
//! the port — two monotonic sources with different epochs in one image, under a
//! doc claiming they were one. W10 moved the executor onto the port whenever a
//! port exists; this module was not moved with it. Two clocks are only harmless
//! while nobody compares them, and the whole purpose of this module is `dt`
//! across callbacks the executor scheduled.

#![allow(clippy::module_name_repetitions)]

use core::time::Duration;

/// Monotonic time since an unspecified epoch.
///
/// ```ignore
/// let t0 = nros::time::now();
/// // ... work ...
/// let dt = nros::time::now() - t0;
/// ```
///
/// Reads the platform port's `nros_platform_clock_ns` (RFC-0073) — on a hosted
/// build too, which is the point: this and the executor must be ONE clock. What
/// the low digits are worth is per-port; ask
/// `nros_platform_clock_resolution_ns`.
#[cfg(feature = "rmw-cffi")]
#[must_use]
pub fn now() -> Duration {
    unsafe extern "C" {
        fn nros_platform_clock_ns() -> u64;
    }
    // SAFETY: bare query of the platform's monotonic us counter; the
    // symbol is guaranteed by whichever platform port linked the
    // binary (the same contract the executor's default timer clock
    // depends on).
    Duration::from_nanos(unsafe { nros_platform_clock_ns() })
}

/// Convenience: [`now`] as whole microseconds.
#[cfg(feature = "rmw-cffi")]
#[must_use]
pub fn now_us() -> u64 {
    u64::try_from(now().as_micros()).unwrap_or(u64::MAX)
}
