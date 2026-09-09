// Copyright (c) 2026, NEWSLab NTU.
// SPDX-License-Identifier: Apache-2.0

//! phase-436 W7 — installing a port's park primitive, in one place.
//!
//! Every Rust board entry that wires the seam needs the same three steps and
//! the same non-obvious constraint, so they live here rather than being spelt
//! once per board and drifting.
//!
//! **The constraint.** A `ParkUntilFn` must wait on the executor's OWN wake
//! object. The backend's listener signals that one, and [`spin_once`] drops
//! its transport drain to non-blocking once a port has parked — so a park on
//! any other object cannot be broken by data arriving, and sleeps out its full
//! deadline with a message already waiting. That is worse than not parking at
//! all, it passes every test, and it is invisible in a quiet run.
//!
//! [`spin_once`]: super::spin::Executor::spin_once

#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
use super::spin::{Executor, ParkUntilFn};

/// Install `park` on `executor`, waiting on the executor's own wake object.
///
/// Returns `false` — and installs nothing — when this build has no wake
/// object to park on. That build keeps the millisecond `wake_wait_ms` path it
/// already had, which is why every caller can treat this as advisory.
///
/// `granularity_us` is the finest park the primitive can express. The PORT
/// states it, because only the port knows: an RTOS tick is a coarser limit
/// than the ABI signature (ThreadX ticks at 100 Hz) and a timespec primitive a
/// finer one (issue 1242).
#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
pub fn install_port_park(
    executor: &mut Executor<'_>,
    park: ParkUntilFn,
    granularity_us: u64,
) -> bool {
    let wake = executor.wake_raw_ptr();
    if wake.is_null() {
        return false;
    }
    executor.set_park_primitive(park, wake, granularity_us);
    true
}
