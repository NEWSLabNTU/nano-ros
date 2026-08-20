//! Clock API for nros
//!
//! This module provides clock abstraction for different time sources:
//! - **SystemTime**: Wall clock time (affected by system time changes)
//! - **SteadyTime**: Monotonic time (not affected by system time changes)
//! - **RosTime**: Simulation time (can be paused/scaled)
//!
//! # Example
//!
//! ```text
//! use nros::clock::{Clock, ClockType};
//!
//! // Create a system clock
//! let clock = Clock::system();
//! let now = clock.now();
//! println!("Current time: {} sec", now.sec);
//!
//! // Create a steady clock for measuring durations
//! let clock = Clock::steady();
//! let start = clock.now();
//! // ... do work ...
//! let elapsed = clock.now() - start;
//! ```
//!
//! # no_std Support
//!
//! Without the `std` feature, clocks return time based on an internal
//! counter that must be updated manually via `update_time()`. This is
//! suitable for embedded systems with RTIC or bare-metal polling loops.

use crate::time::Time;

// AtomicI64 is not available on all platforms (e.g., thumbv7em-none-eabihf)
// Use AtomicI64 when available, otherwise use a simpler approach
#[cfg(target_has_atomic = "64")]
use core::sync::atomic::{AtomicI64, Ordering};

// For platforms without 64-bit atomics, use two 32-bit values
#[cfg(not(target_has_atomic = "64"))]
use core::sync::atomic::{AtomicI32, Ordering};

/// Type of clock to use for time queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClockType {
    /// System time (wall clock)
    ///
    /// This clock reflects the system's real-time clock and may be
    /// affected by NTP adjustments, user changes, or daylight saving time.
    /// Use for timestamps that need to correlate with real-world time.
    #[default]
    SystemTime,

    /// Steady/monotonic time
    ///
    /// This clock is guaranteed to be monotonically increasing and is
    /// not affected by system time changes. Use for measuring durations
    /// and timeouts.
    SteadyTime,

    /// ROS time (simulation time)
    ///
    /// This clock can be overridden for simulation purposes. When a
    /// ROS time override is active, `now()` returns the overridden time.
    /// Otherwise, it falls back to system time.
    RosTime,
}

// On platforms with 64-bit atomics, use AtomicI64 directly
#[cfg(target_has_atomic = "64")]
mod atomic_time {
    use super::*;

    /// Global ROS time override (nanoseconds since epoch)
    /// When set to a non-negative value, `Clock::now()` for `RosTime` clocks
    /// will return this value instead of system time.
    pub(super) static ROS_TIME_OVERRIDE_NANOS: AtomicI64 = AtomicI64::new(-1);

    /// Global steady time counter (nanoseconds)
    /// For `no_std` environments, this counter must be updated manually.
    pub(super) static STEADY_TIME_NANOS: AtomicI64 = AtomicI64::new(0);

    pub(super) fn get_ros_override() -> i64 {
        ROS_TIME_OVERRIDE_NANOS.load(Ordering::Relaxed)
    }

    pub(super) fn set_ros_override(nanos: i64) {
        ROS_TIME_OVERRIDE_NANOS.store(nanos, Ordering::Relaxed);
    }

    pub(super) fn get_steady() -> i64 {
        STEADY_TIME_NANOS.load(Ordering::Relaxed)
    }

    pub(super) fn set_steady(nanos: i64) {
        STEADY_TIME_NANOS.store(nanos, Ordering::Relaxed);
    }

    pub(super) fn add_steady(delta: i64) {
        STEADY_TIME_NANOS.fetch_add(delta, Ordering::Relaxed);
    }
}

// On platforms without 64-bit atomics, use split 32-bit values
// Note: This is not fully atomic but works for single-threaded embedded contexts
#[cfg(not(target_has_atomic = "64"))]
mod atomic_time {
    use super::*;

    // Split into high and low 32-bit parts
    static ROS_TIME_OVERRIDE_LOW: AtomicI32 = AtomicI32::new(-1);
    static ROS_TIME_OVERRIDE_HIGH: AtomicI32 = AtomicI32::new(-1);
    static STEADY_TIME_LOW: AtomicI32 = AtomicI32::new(0);
    static STEADY_TIME_HIGH: AtomicI32 = AtomicI32::new(0);

    pub(super) fn get_ros_override() -> i64 {
        let high = ROS_TIME_OVERRIDE_HIGH.load(Ordering::Relaxed);
        let low = ROS_TIME_OVERRIDE_LOW.load(Ordering::Relaxed);
        if high < 0 {
            -1
        } else {
            ((high as i64) << 32) | (low as u32 as i64)
        }
    }

    pub(super) fn set_ros_override(nanos: i64) {
        if nanos < 0 {
            ROS_TIME_OVERRIDE_HIGH.store(-1, Ordering::Relaxed);
            ROS_TIME_OVERRIDE_LOW.store(-1, Ordering::Relaxed);
        } else {
            ROS_TIME_OVERRIDE_HIGH.store((nanos >> 32) as i32, Ordering::Relaxed);
            ROS_TIME_OVERRIDE_LOW.store(nanos as i32, Ordering::Relaxed);
        }
    }

    pub(super) fn get_steady() -> i64 {
        let high = STEADY_TIME_HIGH.load(Ordering::Relaxed);
        let low = STEADY_TIME_LOW.load(Ordering::Relaxed);
        ((high as i64) << 32) | (low as u32 as i64)
    }

    pub(super) fn set_steady(nanos: i64) {
        STEADY_TIME_HIGH.store((nanos >> 32) as i32, Ordering::Relaxed);
        STEADY_TIME_LOW.store(nanos as i32, Ordering::Relaxed);
    }

    pub(super) fn add_steady(delta: i64) {
        let current = get_steady();
        set_steady(current.saturating_add(delta));
    }
}

/// A clock for querying time
///
/// Clocks provide access to different time sources. Each node typically
/// has an associated clock, but you can also create standalone clocks.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    clock_type: ClockType,
}

impl Default for Clock {
    fn default() -> Self {
        Self::system()
    }
}

/// The platform's wall clock, or `None` when this build has no port to ask.
///
/// phase-359 W10 (backend tier). `nros-core` sits BELOW `nros-platform`, so it
/// cannot depend on it — it declares the two ABI symbols directly, exactly as
/// `nros-node` already does for `nros_platform_clock_ns` ("every platform port
/// exports it through the same linkage contract"). The feature is what promises
/// a port is linked; without it this is `None` and the caller keeps the counter
/// it had.
///
/// ONE symbol since issue 0532 item 5 collapsed the wall clock; this function
/// was named there as the one place that would change, and it was.
// phase-359 W10 follow-up — NOT `not(std)`. This used to be gated away on a
// `std` build, so an image with a port linked read `SystemTime` from
// `Clock::system()` and `nros_platform_time_now_ns` from the executor's epoch
// source: two wall clocks, one image. They agree on POSIX by coincidence (both
// are CLOCK_REALTIME) and stop agreeing the moment a port has an opinion — an
// RTC-backed or simulated one — because only the port is authoritative and only
// one of the two readers asked it. The rule W10 set in `nros-node` applies
// here: when a port is linked it IS the clock, and `std` is what a build
// without one falls back to.
#[cfg(feature = "platform-clock")]
fn platform_wall_clock() -> Option<Time> {
    unsafe extern "C" {
        fn nros_platform_time_now_ns() -> u64;
    }
    // SAFETY: a bare wall-clock read, no pointer arguments, guaranteed by
    // whichever port linked the binary — the same contract `nros-node` relies
    // on for `nros_platform_clock_ns`.
    //
    // ONE symbol, so one sample: issue 0532 collapsed the former
    // `time_since_epoch_{secs,nanos}` pair, which this was written against and
    // which needed a bounded re-read to survive a second boundary landing
    // between the two calls. That loop is what the collapse deletes.
    let ns = unsafe { nros_platform_time_now_ns() };
    // A port with no RTC returns 0. Reporting the Unix epoch as "now" would be
    // a wrong answer stated confidently, so say nothing and let the caller's
    // counter fallback stand.
    if ns == 0 {
        return None;
    }
    Some(Time::new(
        (ns / 1_000_000_000) as i32,
        (ns % 1_000_000_000) as u32,
    ))
}

#[cfg(not(feature = "platform-clock"))]
fn platform_wall_clock() -> Option<Time> {
    None
}

impl Clock {
    /// Create a new clock of the specified type
    pub const fn new(clock_type: ClockType) -> Self {
        Self { clock_type }
    }

    /// Create a system time clock
    ///
    /// System time reflects the real-world wall clock time.
    pub const fn system() -> Self {
        Self {
            clock_type: ClockType::SystemTime,
        }
    }

    /// Create a steady (monotonic) time clock
    ///
    /// Steady time is guaranteed to only increase and is not affected
    /// by system time changes.
    pub const fn steady() -> Self {
        Self {
            clock_type: ClockType::SteadyTime,
        }
    }

    /// Create a ROS time clock
    ///
    /// ROS time can be overridden for simulation. When no override is
    /// active, it returns system time.
    pub const fn ros_time() -> Self {
        Self {
            clock_type: ClockType::RosTime,
        }
    }

    /// Get the clock type
    pub const fn clock_type(&self) -> ClockType {
        self.clock_type
    }

    /// Get the current time from this clock
    ///
    /// # Platform behavior
    ///
    /// - **A platform port linked** (`platform-clock`): the port's wall clock,
    ///   on either flavour — it is the authority, and an image must not hold
    ///   two answers to "what time is it".
    /// - **No port**: the internal counter, which the caller advances with
    ///   `update_steady_time()`.
    ///
    /// phase-359 W10 — there is no `std::time` arm. The platform API IS the
    /// clock: a build with a port reads it, a build without one has no clock to
    /// read and says so with the counter it was given. `SystemTime` used to sit
    /// here as a third answer for hosted builds, which made "what time is it"
    /// depend on whether some crate in the graph happened to name `std`.
    ///
    /// `SteadyTime` is the counter on every flavour, deliberately: it is
    /// advanced by its owner rather than read from a source, and the port's
    /// monotonic export (`nros_platform_clock_ns`) is the executor's business,
    /// not this type's.
    pub fn now(&self) -> Time {
        match self.clock_type {
            ClockType::SystemTime => {
                // phase-359 W10 (backend tier) — a wall clock that does not
                // need the `std` FEATURE. Before this, `SystemTime` here fell
                // back to the STEADY counter: the same value a monotonic clock
                // returns, presented as time since the Unix epoch. That is not
                // a degraded wall clock, it is a different quantity, and the
                // only thing standing between a build and it was whether some
                // crate in the graph happened to name `std`.
                if let Some(t) = platform_wall_clock() {
                    return t;
                }
                let nanos = atomic_time::get_steady();
                Time::from_nanos(nanos)
            }
            ClockType::SteadyTime => {
                let nanos = atomic_time::get_steady();
                Time::from_nanos(nanos)
            }
            ClockType::RosTime => {
                let override_nanos = atomic_time::get_ros_override();
                if override_nanos >= 0 {
                    Time::from_nanos(override_nanos)
                } else {
                    let nanos = atomic_time::get_steady();
                    Time::from_nanos(nanos)
                }
            }
        }
    }

    /// Set a ROS time override
    ///
    /// When set, all `RosTime` clocks will return this time instead of
    /// system time. This is useful for simulation.
    ///
    /// # Arguments
    /// * `nanos` - Nanoseconds since epoch
    pub fn set_ros_time_override(nanos: i64) {
        atomic_time::set_ros_override(nanos);
    }

    /// Set a ROS time override from a Time value
    pub fn set_ros_time_override_time(time: Time) {
        Self::set_ros_time_override(time.to_nanos());
    }

    /// Clear the ROS time override
    ///
    /// After clearing, `RosTime` clocks will return system time again.
    pub fn clear_ros_time_override() {
        atomic_time::set_ros_override(-1);
    }

    /// Check if a ROS time override is active
    pub fn is_ros_time_override_active() -> bool {
        atomic_time::get_ros_override() >= 0
    }

    /// Get the current ROS time override value (if active)
    pub fn get_ros_time_override() -> Option<Time> {
        let nanos = atomic_time::get_ros_override();
        if nanos >= 0 {
            Some(Time::from_nanos(nanos))
        } else {
            None
        }
    }

    /// Update the steady time counter
    ///
    /// For `no_std` environments, call this periodically from your
    /// main loop or RTIC task to advance the steady clock.
    ///
    /// # Arguments
    /// * `delta_nanos` - Nanoseconds elapsed since last call
    pub fn update_steady_time(delta_nanos: i64) {
        atomic_time::add_steady(delta_nanos);
    }

    /// Update the steady time counter (milliseconds version)
    ///
    /// Convenience method for RTIC tasks using millisecond intervals.
    ///
    /// # Arguments
    /// * `delta_ms` - Milliseconds elapsed since last call
    pub fn update_steady_time_ms(delta_ms: u64) {
        let delta_nanos = delta_ms as i64 * 1_000_000;
        Self::update_steady_time(delta_nanos);
    }

    /// Set the steady time counter to a specific value
    ///
    /// Use this to initialize the clock or synchronize with an external
    /// time source.
    pub fn set_steady_time(nanos: i64) {
        atomic_time::set_steady(nanos);
    }

    /// Get the current steady time counter value
    pub fn get_steady_time_nanos() -> i64 {
        atomic_time::get_steady()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_type_default() {
        let clock_type = ClockType::default();
        assert_eq!(clock_type, ClockType::SystemTime);
    }

    #[test]
    fn test_clock_constructors() {
        let system = Clock::system();
        assert_eq!(system.clock_type(), ClockType::SystemTime);

        let steady = Clock::steady();
        assert_eq!(steady.clock_type(), ClockType::SteadyTime);

        let ros = Clock::ros_time();
        assert_eq!(ros.clock_type(), ClockType::RosTime);

        let custom = Clock::new(ClockType::SteadyTime);
        assert_eq!(custom.clock_type(), ClockType::SteadyTime);
    }

    #[test]
    fn test_clock_default() {
        let clock = Clock::default();
        assert_eq!(clock.clock_type(), ClockType::SystemTime);
    }

    #[test]
    fn test_ros_time_override() {
        // Clear any existing override
        Clock::clear_ros_time_override();
        assert!(!Clock::is_ros_time_override_active());
        assert!(Clock::get_ros_time_override().is_none());

        // Set override
        let override_time = Time::new(1234567890, 123456789);
        Clock::set_ros_time_override_time(override_time);
        assert!(Clock::is_ros_time_override_active());
        assert_eq!(Clock::get_ros_time_override(), Some(override_time));

        // ROS clock should return override time
        let ros_clock = Clock::ros_time();
        let now = ros_clock.now();
        assert_eq!(now, override_time);

        // Clear override
        Clock::clear_ros_time_override();
        assert!(!Clock::is_ros_time_override_active());
    }

    #[test]
    fn test_steady_time_update() {
        // Reset steady time
        Clock::set_steady_time(0);
        assert_eq!(Clock::get_steady_time_nanos(), 0);

        // Update by milliseconds
        Clock::update_steady_time_ms(100);
        assert_eq!(Clock::get_steady_time_nanos(), 100_000_000);

        // Update by nanoseconds
        Clock::update_steady_time(500_000_000);
        assert_eq!(Clock::get_steady_time_nanos(), 600_000_000);

        // Steady clock should return updated time
        let steady_clock = Clock::steady();
        let now = steady_clock.now();
        assert_eq!(now.to_nanos(), 600_000_000);
    }

    #[test]
    #[cfg(all(feature = "std", not(feature = "platform-clock")))]
    #[cfg_attr(miri, ignore)] // Miri doesn't support clock_gettime with isolation
    fn test_system_clock_returns_nonzero() {
        let clock = Clock::system();
        let now = clock.now();
        // System time should be after Unix epoch (positive)
        assert!(now.sec > 0);
    }

    /// phase-359 W10 follow-up — a linked port OUTRANKS `std` for the wall
    /// clock, on a `std` build too.
    ///
    /// This is the one configuration where the two disagree observably, and it
    /// is why the test defines the port symbol itself: the value returned is
    /// nothing like a real `SystemTime`, so a `Clock::system()` that answered
    /// with `SystemTime::now()` — which is what this file did before, because
    /// `platform_wall_clock` was gated `not(std)` — fails loudly rather than
    /// coincidentally passing. On POSIX both sources are CLOCK_REALTIME, so
    /// nothing short of a port with an opinion can tell them apart.
    #[test]
    #[cfg(all(feature = "std", feature = "platform-clock"))]
    fn platform_port_outranks_std_for_the_wall_clock() {
        /// 2001-09-09T01:46:40Z — a fixed instant no real clock will return.
        const FAKE_NS: u64 = 1_000_000_000 * 1_000_000_000;

        #[unsafe(no_mangle)]
        extern "C" fn nros_platform_time_now_ns() -> u64 {
            FAKE_NS
        }

        let now = Clock::system().now();
        assert_eq!(
            now.sec, 1_000_000_000,
            "Clock::system() must read the linked port, not SystemTime"
        );
        assert_eq!(now.nanosec, 0);
    }
}
