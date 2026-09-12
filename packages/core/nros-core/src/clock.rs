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
//! # Where the time comes from
//!
//! With a platform port linked (the `platform-clock` feature), every clock
//! reads the PORT: `nros_platform_time_now_ns` for the wall, and
//! `nros_platform_clock_ns` for the monotonic. Without one there is no clock
//! to read, and all three types fall back to an internal counter the caller
//! advances with `update_steady_time()` — suitable for a bare-metal RTIC or
//! polling loop, which is the only build where that counter has an owner.

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
    /// and timeouts. Reads the port's `nros_platform_clock_ns` where one is
    /// linked, and the caller-advanced counter where none is.
    SteadyTime,

    /// ROS time (simulation time)
    ///
    /// This clock can be overridden for simulation purposes. When a
    /// ROS time override is active — a simulator's or a bag player's
    /// `/clock` — `now()` returns the overridden time. Otherwise it IS
    /// [`SystemTime`](Self::SystemTime), which is what `rclcpp`'s
    /// `ClockType::ROS_TIME` reads with `use_sim_time` false, so a node
    /// written for simulation still runs standalone.
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

/// The platform's MONOTONIC clock, or `None` when this build has no port to ask.
///
/// issue 1334 — the sibling of [`platform_wall_clock`], declared the same way
/// and under the same feature, because it is the same promise: `platform-clock`
/// means a port is linked, and every port exports both halves
/// (`nros/platform.h` "Wall clock" and RFC-0073's monotonic export are
/// MANDATORY, not optional). `nros-node`, `nros-log`, `nros-c` and the `nros`
/// umbrella already hand-declare this symbol under exactly this contract.
///
/// Unlike the wall clock there is no "port with no RTC" case to detect: a
/// monotonic counter reading 0 is a legitimate answer at boot, so a zero is
/// passed through rather than read as "no clock".
#[cfg(feature = "platform-clock")]
fn platform_steady_clock() -> Option<Time> {
    unsafe extern "C" {
        fn nros_platform_clock_ns() -> u64;
    }
    // SAFETY: a bare counter read, no pointer arguments, guaranteed by
    // whichever port linked the binary — the same contract `platform_wall_clock`
    // above relies on for `nros_platform_time_now_ns`.
    let ns = unsafe { nros_platform_clock_ns() };
    Some(Time::new(
        (ns / 1_000_000_000) as i32,
        (ns % 1_000_000_000) as u32,
    ))
}

#[cfg(not(feature = "platform-clock"))]
fn platform_steady_clock() -> Option<Time> {
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
    /// active, it returns system time — the identical expression
    /// [`Clock::system`] evaluates, so the two cannot drift (issue 1334).
    pub const fn ros_time() -> Self {
        Self {
            clock_type: ClockType::RosTime,
        }
    }

    /// Get the clock type
    pub const fn clock_type(&self) -> ClockType {
        self.clock_type
    }

    /// The wall clock, in ONE place — issue 1334.
    ///
    /// Two clock types answer "what is the wall time", and they must answer it
    /// identically: `SystemTime` always, and `RosTime` whenever no `/clock`
    /// override is installed. Before this they were two expressions, and only
    /// the first was ever moved onto the port (phase-359 W10), so `RosTime`'s
    /// fallback was left reading the counter — a value nothing in the tree
    /// advances. One expression is what keeps the next move from splitting
    /// them again.
    ///
    /// - **A platform port linked** (`platform-clock`): the port's wall clock.
    ///   It is the authority, and an image must not hold two answers to "what
    ///   time is it".
    /// - **No port**: the internal counter, which the caller advances with
    ///   `update_steady_time()`. A build with no port has no clock to read, so
    ///   this is the honest answer rather than a fallback with an opinion.
    fn wall_now() -> Time {
        // phase-359 W10 (backend tier) — a wall clock that does not need the
        // `std` FEATURE. Before this, `SystemTime` fell back to the STEADY
        // counter unconditionally: the same value a monotonic clock returns,
        // presented as time since the Unix epoch. That is not a degraded wall
        // clock, it is a different quantity, and the only thing standing
        // between a build and it was whether some crate in the graph happened
        // to name `std`.
        if let Some(t) = platform_wall_clock() {
            return t;
        }
        Time::from_nanos(atomic_time::get_steady())
    }

    /// The monotonic clock, in ONE place — issue 1334's sibling site.
    ///
    /// Same rule as [`Self::wall_now`], one clock over: a linked port IS the
    /// monotonic clock, and the counter is what a build with no port was
    /// given.
    ///
    /// This overturns half of phase-359 W10, which moved `SystemTime` onto the
    /// port and left this line reading "`SteadyTime` is the counter on every
    /// flavour, deliberately: it is advanced by its owner … and the port's
    /// monotonic export is the executor's business, not this type's". The
    /// owner never arrived — nothing outside this file's own tests has ever
    /// called `update_steady_time` — so on every shipped image
    /// `Clock::steady().now()` was the constant 0, while the C surface
    /// answered the SAME question (`NROS_CLOCK_STEADY_TIME`) with
    /// `nros_platform_clock_ns`. That is issue 1334's complaint exactly, one
    /// clock over: two surfaces, one image, different answers, and the Rust
    /// one is not a clock. W10's own rule settles it — when a port is linked
    /// it IS the clock — and the counter keeps its documented RTIC meaning in
    /// the one build where it is the only source.
    fn steady_now() -> Time {
        if let Some(t) = platform_steady_clock() {
            return t;
        }
        Time::from_nanos(atomic_time::get_steady())
    }

    /// Get the current time from this clock
    ///
    /// # Platform behavior
    ///
    /// - **A platform port linked** (`platform-clock`): the port's clocks —
    ///   `nros_platform_time_now_ns` for the wall, `nros_platform_clock_ns`
    ///   for the monotonic. The port is the authority, and an image must not
    ///   hold two answers to "what time is it".
    /// - **No port**: the internal counter, which the caller advances with
    ///   `update_steady_time()`.
    ///
    /// phase-359 W10 — there is no `std::time` arm. The platform API IS the
    /// clock: a build with a port reads it, a build without one has no clock to
    /// read and says so with the counter it was given. `SystemTime` used to sit
    /// here as a third answer for hosted builds, which made "what time is it"
    /// depend on whether some crate in the graph happened to name `std`.
    ///
    /// `RosTime` with no override is `SystemTime`, in EVERY build shape and by
    /// construction — both arms call [`Self::wall_now`]. That is rclcpp's
    /// contract (`ClockType::ROS_TIME` with `use_sim_time` false reads the
    /// system clock), what five doc sites across three languages already
    /// promised, and what the C surface already did; issue 1334 is the four
    /// phases in which the Rust arm said something else.
    pub fn now(&self) -> Time {
        match self.clock_type {
            ClockType::SystemTime => Self::wall_now(),
            ClockType::SteadyTime => Self::steady_now(),
            ClockType::RosTime => {
                let override_nanos = atomic_time::get_ros_override();
                if override_nanos >= 0 {
                    // A `/clock` source is driving simulated time.
                    Time::from_nanos(override_nanos)
                } else {
                    // None is. rclcpp reads the system clock here and so do we
                    // — the same expression `Clock::system()` evaluates, not a
                    // copy of it (issue 1334).
                    Self::wall_now()
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
    /// For environments with NO platform port, call this periodically from
    /// your main loop or RTIC task to advance the clocks. With a port linked
    /// this counter is not what any clock reads — the port is (issue 1334) —
    /// and advancing it has no observable effect.
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

    /// Serialises every test that touches the process-global clock state —
    /// issue 1334.
    ///
    /// The override, the counter and (below) the fake port are all statics, and
    /// `cargo test` runs these in parallel threads: a test that SETS one and
    /// reads it back is racing every sibling that writes the same static. That
    /// was already true before this guard existed (`test_ros_time_override` and
    /// `test_steady_time_update` write two different globals that `Clock::now`
    /// reads together) and it becomes load-bearing here, because the new tests
    /// move the fake port's hands while another test asserts where they point.
    ///
    /// A spin lock rather than `std::sync::Mutex`: this crate is `#![no_std]`
    /// and `std` is a FEATURE, so a `Mutex` here would confine the guard — and
    /// therefore every test using it — to the `std` flavour, which is not where
    /// the `platform-clock` tests need to live.
    struct ClockGuard;

    static CLOCK_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

    impl ClockGuard {
        fn acquire() -> Self {
            while CLOCK_LOCK
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                core::hint::spin_loop();
            }
            Self
        }
    }

    impl Drop for ClockGuard {
        fn drop(&mut self) {
            // Leave the globals as a fresh process has them, so a test that
            // does NOT take the guard still sees the documented defaults.
            Clock::clear_ros_time_override();
            Clock::set_steady_time(0);
            #[cfg(feature = "platform-clock")]
            fake_port::reset();
            CLOCK_LOCK.store(false, Ordering::Release);
        }
    }

    /// The stand-in platform port for the `platform-clock` flavour.
    ///
    /// One definition of each symbol, at module level, because a linker sees
    /// one test binary: the wall symbol used to be defined INSIDE the single
    /// test that needed it, which works exactly until a second test needs it
    /// too. Both hands MOVE (they are atomics, not constants), which is what
    /// lets a test assert that a clock ADVANCES rather than merely that it
    /// reads a magic number.
    #[cfg(feature = "platform-clock")]
    mod fake_port {
        use core::sync::atomic::{AtomicU64, Ordering};

        /// 2001-09-09T01:46:40Z — a fixed instant no real clock will return.
        pub(super) const WALL_BASE_NS: u64 = 1_000_000_000 * 1_000_000_000;
        /// 42 s of uptime — no relation to `WALL_BASE_NS`, deliberately: the
        /// two clocks are different quantities and a test must not be able to
        /// pass by reading the wrong one.
        pub(super) const STEADY_BASE_NS: u64 = 42_000_000_000;

        pub(super) static WALL_NS: AtomicU64 = AtomicU64::new(WALL_BASE_NS);
        pub(super) static STEADY_NS: AtomicU64 = AtomicU64::new(STEADY_BASE_NS);

        pub(super) fn reset() {
            WALL_NS.store(WALL_BASE_NS, Ordering::Relaxed);
            STEADY_NS.store(STEADY_BASE_NS, Ordering::Relaxed);
        }

        #[unsafe(no_mangle)]
        extern "C" fn nros_platform_time_now_ns() -> u64 {
            WALL_NS.load(Ordering::Relaxed)
        }

        #[unsafe(no_mangle)]
        extern "C" fn nros_platform_clock_ns() -> u64 {
            STEADY_NS.load(Ordering::Relaxed)
        }
    }

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
        let _guard = ClockGuard::acquire();
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
        let _guard = ClockGuard::acquire();
        // Reset steady time
        Clock::set_steady_time(0);
        assert_eq!(Clock::get_steady_time_nanos(), 0);

        // Update by milliseconds
        Clock::update_steady_time_ms(100);
        assert_eq!(Clock::get_steady_time_nanos(), 100_000_000);

        // Update by nanoseconds
        Clock::update_steady_time(500_000_000);
        assert_eq!(Clock::get_steady_time_nanos(), 600_000_000);

        // Steady clock should return updated time -- in the build where the
        // counter is the only clock there is. With a port linked the port
        // outranks it (issue 1334), which is the test below.
        #[cfg(not(feature = "platform-clock"))]
        {
            let steady_clock = Clock::steady();
            let now = steady_clock.now();
            assert_eq!(now.to_nanos(), 600_000_000);
        }
    }

    /// With NO port linked there is no clock, and all three types say so with
    /// the one counter they were given — issue 1334.
    ///
    /// This replaces `test_system_clock_returns_nonzero`, which asserted
    /// `now.sec > 0` for `Clock::system()` in exactly this shape. That claim
    /// was false from the day phase-359 W10 removed the `std::time` arm — a
    /// portless build reads the counter, which is 0 — and nobody saw it,
    /// because its `all(std, not(platform-clock))` gate put it in no lane at
    /// all: the one nros-core feature lane is `std,platform-clock`, and the
    /// default lane has no `std`. A test in no lane is the shape CLAUDE.md's
    /// `required-features` rule names, one cfg over.
    ///
    /// What it asserts instead is the property that IS true here, and it is the
    /// portless half of this issue's fix: `RosTime` with no override reads
    /// whatever `SystemTime` reads, whatever that turns out to be.
    #[test]
    #[cfg(not(feature = "platform-clock"))]
    fn without_a_port_every_clock_is_the_counter() {
        let _guard = ClockGuard::acquire();
        Clock::clear_ros_time_override();
        Clock::set_steady_time(7_500_000_000);

        assert_eq!(Clock::system().now().to_nanos(), 7_500_000_000);
        assert_eq!(Clock::steady().now().to_nanos(), 7_500_000_000);
        assert_eq!(
            Clock::ros_time().now().to_nanos(),
            7_500_000_000,
            "with no override and no port, ROS time is what system time is"
        );
    }

    /// phase-359 W10 follow-up — a linked port OUTRANKS `std` for the wall
    /// clock, on a `std` build too.
    ///
    /// This is the one configuration where the two disagree observably, and it
    /// is why the test image defines the port symbol itself (`fake_port`): the
    /// value returned is nothing like a real `SystemTime`, so a
    /// `Clock::system()` that answered with `SystemTime::now()` — which is what
    /// this file did before, because `platform_wall_clock` was gated
    /// `not(std)` — fails loudly rather than coincidentally passing. On POSIX
    /// both sources are CLOCK_REALTIME, so nothing short of a port with an
    /// opinion can tell them apart.
    #[test]
    #[cfg(all(feature = "std", feature = "platform-clock"))]
    fn platform_port_outranks_std_for_the_wall_clock() {
        let _guard = ClockGuard::acquire();
        let now = Clock::system().now();
        assert_eq!(
            now.sec, 1_000_000_000,
            "Clock::system() must read the linked port, not SystemTime"
        );
        assert_eq!(now.nanosec, 0);
    }

    /// ROS time with no `/clock` source is the WALL clock, and it RUNS — issue
    /// 1334, and the negative control for the whole fix.
    ///
    /// Before the fix this arm read `STEADY_TIME_NANOS`, a counter whose own
    /// doc says it is "advanced by its owner" and which nothing outside this
    /// file's tests has ever advanced. So the assertion that fails on the
    /// pre-fix tree is the first one (the reading was 0, not the port's wall
    /// clock) and the one after it (a clock that does not move is not a
    /// clock).
    ///
    /// `rclcpp`'s `ClockType::ROS_TIME` with `use_sim_time` false reads the
    /// system clock; five doc sites in this tree across Rust, C and C++ said we
    /// did too; and the C surface (`nros_clock_get_now(NROS_CLOCK_ROS_TIME)`)
    /// always did. This is the arm that disagreed with all of them.
    #[test]
    #[cfg(feature = "platform-clock")]
    fn ros_time_with_no_override_reads_the_port_wall_clock() {
        let _guard = ClockGuard::acquire();
        Clock::clear_ros_time_override();
        // A counter reading that is nothing like the wall clock, so reading the
        // wrong source cannot pass by coincidence.
        Clock::set_steady_time(1_234);

        let ros = Clock::ros_time().now();
        assert_eq!(
            ros,
            Clock::system().now(),
            "with no override, ROS time IS system time"
        );
        assert_eq!(ros.sec, 1_000_000_000, "it must be the PORT's wall clock");

        // ...and it advances with it. Two seconds on the port's hand.
        fake_port::WALL_NS.fetch_add(2_000_000_000, core::sync::atomic::Ordering::Relaxed);
        let later = Clock::ros_time().now();
        assert_eq!(
            later.to_nanos() - ros.to_nanos(),
            2_000_000_000,
            "ROS time with no source must ADVANCE with the wall clock"
        );
    }

    /// A `/clock` override still wins over the wall clock — the sim-time path
    /// this fix must not touch.
    #[test]
    #[cfg(feature = "platform-clock")]
    fn an_override_still_outranks_the_wall_clock() {
        let _guard = ClockGuard::acquire();
        Clock::set_ros_time_override(5_000_000_000);
        assert_eq!(Clock::ros_time().now().to_nanos(), 5_000_000_000);
        assert_ne!(
            Clock::ros_time().now(),
            Clock::system().now(),
            "a simulated clock must not be shadowed by the wall clock"
        );
        Clock::clear_ros_time_override();
        assert_eq!(Clock::ros_time().now(), Clock::system().now());
    }

    /// The sibling site: a linked port outranks the counter for the MONOTONIC
    /// clock too — issue 1334's sweep.
    ///
    /// Fails on the pre-fix tree, where `SteadyTime` read the counter on every
    /// flavour and therefore returned 0 in every shipped image, while the C
    /// surface answered `NROS_CLOCK_STEADY_TIME` with `nros_platform_clock_ns`.
    #[test]
    #[cfg(feature = "platform-clock")]
    fn platform_port_outranks_the_counter_for_the_steady_clock() {
        let _guard = ClockGuard::acquire();
        Clock::set_steady_time(1_234);

        let now = Clock::steady().now();
        assert_eq!(
            now.to_nanos(),
            fake_port::STEADY_BASE_NS as i64,
            "Clock::steady() must read the linked port, not the counter"
        );
        assert_ne!(
            now.to_nanos(),
            Clock::system().now().to_nanos(),
            "the monotonic clock is a different quantity from the wall clock"
        );

        fake_port::STEADY_NS.fetch_add(250_000_000, core::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            Clock::steady().now().to_nanos() - now.to_nanos(),
            250_000_000
        );
    }
}
