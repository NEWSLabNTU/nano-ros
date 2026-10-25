//! Clock API for nros C API.
//!
//! Provides time sources for ROS 2 compatible timing operations.

use core::ffi::c_int;

use crate::error::*;

// ============================================================================
// Time Types
// ============================================================================

/// Time representation compatible with builtin_interfaces/msg/Time.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct rclc_time_t {
    /// Seconds component
    pub sec: i32,
    /// Nanoseconds component (0 to 999,999,999)
    pub nanosec: u32,
}

/// Duration representation compatible with builtin_interfaces/msg/Duration.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct rcl_duration_t {
    /// Seconds component (can be negative)
    pub sec: i32,
    /// Nanoseconds component (0 to 999,999,999)
    pub nanosec: u32,
}

// ============================================================================
// Clock Types
// ============================================================================

/// Clock type enumeration.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rcl_clock_type_t {
    /// Uninitialized clock
    RCL_CLOCK_UNINITIALIZED = 0,
    /// ROS time - follows /clock topic if available, otherwise system time
    RCL_ROS_TIME = 1,
    /// System time - wall clock time from the operating system
    RCL_SYSTEM_TIME = 2,
    /// Steady time - monotonic clock, not affected by system time changes
    RCL_STEADY_TIME = 3,
}

/// phase-430 W1 — the ONE mapping from a C clock kind to the executor's
/// timer clock source, and the reason it lives here rather than in
/// `nros-node`: these discriminants belong to `rcl_clock_type_t`, so a copy
/// of them a crate away is the hand-mirror drift 0135/0160 measured one layer
/// down. Both FFI crates call this — `rcl_timer_init` (C) and
/// `nros_cpp_timer_create_on_clock` (C++) — so a clock kind cannot mean one
/// thing in a C image and another in a C++ one.
///
/// `None` for a kind that names no source: `RCL_CLOCK_UNINITIALIZED`, and any
/// out-of-range byte a C caller widened into the parameter. Both are REJECTED
/// by the callers rather than defaulted to `Steady` — a timer created on a
/// clock the caller never initialised would silently be a wall timer, which is
/// the exact confusion phase-425 exists to remove.
///
/// issue 1177 -- GATED on `rmw-cffi`, and the gate follows the TYPE. The return
/// type `nros_node::executor::TimerClockSource` is re-exported behind
/// `#[cfg(any(has_rmw, test))]`, and `has_rmw` is set from
/// `CARGO_FEATURE_RMW_CFFI` -- so without an RMW seam the type does not exist
/// and this signature does not resolve:
///
/// ```text
/// error[E0432]: unresolved import `nros_node::executor::TimerClockSource`
///     --> packages/api/nros-c/src/clock.rs:65:9
/// ```
///
/// `clock` is one of this crate's backend-INDEPENDENT modules, so it is
/// compiled under `nros-c`'s bare default (`panic-platform`), where every
/// CALLER is absent: `executor` and `timer` here, and `nros-cpp`'s `timer`, are
/// all `#[cfg(feature = "rmw-cffi")]` modules. A gated item with an ungated
/// caller is the shape issues 1175 and 1177 both name; this is the same defect
/// with the two sides swapped -- the caller is gated and the callee was not.
#[cfg(feature = "rmw-cffi")]
pub fn nros_timer_clock_source(clock_type: u8) -> Option<nros_node::executor::TimerClockSource> {
    use nros_node::executor::TimerClockSource;

    match clock_type {
        x if x == rcl_clock_type_t::RCL_STEADY_TIME as u8 => Some(TimerClockSource::Steady),
        x if x == rcl_clock_type_t::RCL_ROS_TIME as u8 => Some(TimerClockSource::Ros),
        x if x == rcl_clock_type_t::RCL_SYSTEM_TIME as u8 => Some(TimerClockSource::System),
        _ => None,
    }
}

/// The `rcl_clock_type_t` discriminant a `TimerClockSource` is spelled as --
/// the inverse of [`nros_timer_clock_source`], kept beside it so the two cannot
/// disagree about what a clock kind means. issue 1975: `nros-cpp` reads a
/// timer's clock back for `rclcpp::TimerBase::get_clock()`. Gated like its
/// sibling, for the same reason (issue 1177).
#[cfg(feature = "rmw-cffi")]
pub fn nros_timer_clock_type(source: nros_node::executor::TimerClockSource) -> u8 {
    use nros_node::executor::TimerClockSource;

    match source {
        TimerClockSource::Steady => rcl_clock_type_t::RCL_STEADY_TIME as u8,
        TimerClockSource::Ros => rcl_clock_type_t::RCL_ROS_TIME as u8,
        TimerClockSource::System => rcl_clock_type_t::RCL_SYSTEM_TIME as u8,
    }
}

/// Clock state enumeration.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_clock_state_t {
    /// Not initialized
    RCLC_CLOCK_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    RCLC_CLOCK_STATE_READY = 1,
    /// Shutdown
    RCLC_CLOCK_STATE_SHUTDOWN = 2,
}

/// Clock structure.
#[repr(C)]
pub struct rcl_clock_t {
    /// Clock type
    pub r#type: rcl_clock_type_t,
    /// Current state
    pub state: rclc_clock_state_t,
    /// Internal: steady clock epoch (nanoseconds since process start)
    pub _steady_epoch_ns: u64,
}

impl Default for rcl_clock_t {
    fn default() -> Self {
        Self {
            r#type: rcl_clock_type_t::RCL_CLOCK_UNINITIALIZED,
            state: rclc_clock_state_t::RCLC_CLOCK_STATE_UNINITIALIZED,
            _steady_epoch_ns: 0,
        }
    }
}

// ============================================================================
// Platform-specific time functions
// ============================================================================

use crate::platform;

/// Nanoseconds per second constant
const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Get system time in nanoseconds since Unix epoch.
fn get_system_time_ns() -> i64 {
    platform::get_system_time_ns()
}

/// Get steady (monotonic) time in nanoseconds.
pub(crate) fn get_steady_time_ns() -> u64 {
    platform::get_time_ns()
}

/// What a `RCL_ROS_TIME` clock reads (issue 0789).
///
/// The ROS time override when one is active — a simulator's or a bag player's
/// `/clock`, installed through `rcl_set_ros_time_override` — and otherwise the
/// system clock, which is what `RCL_ROS_TIME` has always read here and
/// what rcl's ROS-time clock falls back to when the override is disabled.
///
/// The override itself is `nros_core::Clock`'s, not a second copy: the C
/// switches below drive the SAME global a Rust `Clock::ros_time()` reads, so an
/// image whose C nodes and Rust nodes share a process cannot end up with two
/// answers to "what time is it".
///
/// issue 1334 — that last sentence used to carry a parenthesis admitting the
/// two surfaces DID differ, on the fallback rather than the override: Rust's
/// `RosTime` with no override read `nros_core`'s in-image steady counter, which
/// nothing in the tree advances, so `rcl_clock_get_now(RCL_ROS_TIME)`
/// and `Clock::ros_time().now()` gave different answers about the same image in
/// the same state. The Rust arm now evaluates the same wall clock this one
/// does, and `ros_time_agrees_with_the_rust_surface` below is what holds the
/// two together.
fn get_ros_time_ns() -> i64 {
    match nros_core::Clock::get_ros_time_override() {
        Some(t) => t.to_nanos(),
        None => get_system_time_ns(),
    }
}

// ============================================================================
// Clock Functions
// ============================================================================

/// Get a zero-initialized clock.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_clock_get_zero_initialized() -> rcl_clock_t {
    rcl_clock_t::default()
}

/// Initialize a clock — rcl's signature (phase-492 W1.b):
/// `rcl_clock_init(type, &clock, &allocator)`. `allocator` must be NULL or
/// `rcl_get_default_allocator()`'s; a clock allocates nothing.
///
/// # Safety
/// `clock` must be valid for writes; `allocator` NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_clock_init(
    clock_type: rcl_clock_type_t,
    clock: *mut rcl_clock_t,
    allocator: *mut crate::allocator::rcl_allocator_t,
) -> rcl_ret_t {
    if clock.is_null() || !unsafe { crate::allocator::allocator_is_accepted(allocator) } {
        return RCL_RET_INVALID_ARGUMENT;
    }

    // Validate clock type
    match clock_type {
        rcl_clock_type_t::RCL_ROS_TIME
        | rcl_clock_type_t::RCL_SYSTEM_TIME
        | rcl_clock_type_t::RCL_STEADY_TIME => {}
        _ => return RCL_RET_INVALID_ARGUMENT,
    }

    let clock = &mut *clock;

    // Check if already initialized
    if clock.state != rclc_clock_state_t::RCLC_CLOCK_STATE_UNINITIALIZED {
        return RCLC_RET_ALREADY_EXISTS;
    }

    clock.r#type = clock_type;
    clock.state = rclc_clock_state_t::RCLC_CLOCK_STATE_READY;

    // For steady time, record the epoch
    if clock_type == rcl_clock_type_t::RCL_STEADY_TIME {
        clock._steady_epoch_ns = get_steady_time_ns();
    }

    RCL_RET_OK
}

/// Get the current time from a clock as an `rclc_time_t` (seconds +
/// nanoseconds). rcl's `rcl_clock_get_now` writes a nanosecond count; that is
/// [`rcl_clock_get_now`] here, and this is the struct-valued sibling
/// (phase-492 W1.b — it was the one spelled `rcl_clock_get_now`, with a
/// different out-type than rcl's).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_clock_get_now_time(
    clock: *const rcl_clock_t,
    time_out: *mut rclc_time_t,
) -> rcl_ret_t {
    if clock.is_null() || time_out.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let clock = &*clock;

    if clock.state != rclc_clock_state_t::RCLC_CLOCK_STATE_READY {
        return RCL_RET_NOT_INIT;
    }

    let nanos = match clock.r#type {
        rcl_clock_type_t::RCL_ROS_TIME => get_ros_time_ns(),
        rcl_clock_type_t::RCL_SYSTEM_TIME => get_system_time_ns(),
        rcl_clock_type_t::RCL_STEADY_TIME => {
            // Steady time relative to epoch
            get_steady_time_ns() as i64
        }
        _ => return RCL_RET_ERROR,
    };

    *time_out = rclc_time_from_nanoseconds(nanos);
    RCL_RET_OK
}

/// Get the current time from a clock as a nanosecond count — rcl's contract
/// (`rcl_time_point_value_t *`). phase-492 W1.b: was `rclc_clock_get_now_ns`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_clock_get_now(
    clock: *const rcl_clock_t,
    nanoseconds: *mut i64,
) -> rcl_ret_t {
    if clock.is_null() || nanoseconds.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let clock = &*clock;

    if clock.state != rclc_clock_state_t::RCLC_CLOCK_STATE_READY {
        return RCL_RET_NOT_INIT;
    }

    let nanos = match clock.r#type {
        rcl_clock_type_t::RCL_ROS_TIME => get_ros_time_ns(),
        rcl_clock_type_t::RCL_SYSTEM_TIME => get_system_time_ns(),
        rcl_clock_type_t::RCL_STEADY_TIME => get_steady_time_ns() as i64,
        _ => return RCL_RET_ERROR,
    };

    *nanoseconds = nanos;
    RCL_RET_OK
}

/// Check if a clock is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_clock_is_valid(clock: *const rcl_clock_t) -> bool {
    if clock.is_null() {
        return false;
    }

    let clock = &*clock;
    clock.state == rclc_clock_state_t::RCLC_CLOCK_STATE_READY
}

/// Get the clock type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_clock_get_type(clock: *const rcl_clock_t) -> rcl_clock_type_t {
    if clock.is_null() {
        return rcl_clock_type_t::RCL_CLOCK_UNINITIALIZED;
    }

    (*clock).r#type
}

/// Finalize a clock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_clock_fini(clock: *mut rcl_clock_t) -> rcl_ret_t {
    if clock.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let clock = &mut *clock;

    if clock.state == rclc_clock_state_t::RCLC_CLOCK_STATE_UNINITIALIZED {
        return RCL_RET_NOT_INIT;
    }

    clock.state = rclc_clock_state_t::RCLC_CLOCK_STATE_SHUTDOWN;
    clock.r#type = rcl_clock_type_t::RCL_CLOCK_UNINITIALIZED;
    clock._steady_epoch_ns = 0;

    RCL_RET_OK
}

// ============================================================================
// ROS time — driving a C image from a simulator's `/clock` (issue 0789)
// ============================================================================
//
// These are the C face of `nros_core::Clock`'s override switches
// (`set_ros_time_override`, `clear_ros_time_override`,
// `is_ros_time_override_active`, `get_ros_time_override`) and they drive that
// crate's global, not a copy of it — one model per image, whichever language
// installs the time. Names and signatures are rcl's
// (`rcl_enable_ros_time_override` …), so a ported node keeps its spelling.
//
// # The one model difference from rcl, stated once
//
// rcl holds two pieces of state: an ENABLED flag and a VALUE, so setting a
// value while disabled stores it without taking effect. Ours holds one — a
// negative sentinel means "no override" — because that is the model Rust has
// had since the clock shipped and a second one in C is exactly the split issue
// 0789 is about. Consequences, both documented at their entry points:
//
//   * `rcl_set_ros_time_override` also ENABLES. There is no representable
//     state where a value is stored but inert.
//   * `rcl_set_ros_time_override` REJECTS a negative time, which is the
//     sentinel's cost. ROS time is nanoseconds since the epoch, so it is
//     non-negative for any clock a simulator or a bag publishes.
//
// The override is process-global rather than per-clock (rcl's lives in the
// clock's storage). With no allocator a clock cannot own a separately-shared
// time source — the same constraint the Rust ledger rows record for
// `Clock::set_ros_time_override` — and an image has one simulation clock.

/// Shared precondition for every ROS-time switch: a live clock, of ROS type.
///
/// `RCLC_RET_NOT_ALLOWED` for the wrong clock TYPE names why the call was
/// refused; rcl returns its generic error there. A steady or system clock is
/// not a thing `/clock` can drive, and silently accepting the call would leave
/// the caller believing the image is simulated when it is reading the wall.
unsafe fn ros_time_clock_ok(clock: *const rcl_clock_t) -> rcl_ret_t {
    if clock.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let clock = &*clock;
    if clock.state != rclc_clock_state_t::RCLC_CLOCK_STATE_READY {
        return RCL_RET_NOT_INIT;
    }
    if clock.r#type != rcl_clock_type_t::RCL_ROS_TIME {
        return RCLC_RET_NOT_ALLOWED;
    }
    RCL_RET_OK
}

/// Enable the ROS time override on a `RCL_ROS_TIME` clock.
///
/// Mirrors `rcl_enable_ros_time_override`. If no override is active yet the
/// clock starts at time 0 — the state rcl reports as "not started"
/// (`rcl_clock_time_started`), i.e. enabled but waiting for the first
/// `/clock` sample. An already-active override keeps its value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_enable_ros_time_override(clock: *const rcl_clock_t) -> rcl_ret_t {
    let ret = ros_time_clock_ok(clock);
    if ret != RCL_RET_OK {
        return ret;
    }

    if !nros_core::Clock::is_ros_time_override_active() {
        nros_core::Clock::set_ros_time_override(0);
    }
    RCL_RET_OK
}

/// Disable the ROS time override; the clock reads the system clock again.
///
/// Mirrors `rcl_disable_ros_time_override`, and `nros_core::Clock::
/// clear_ros_time_override` underneath. The stored time does not survive: with
/// one piece of state there is nowhere for an inert value to live, so
/// re-enabling starts from 0 again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_disable_ros_time_override(clock: *const rcl_clock_t) -> rcl_ret_t {
    let ret = ros_time_clock_ok(clock);
    if ret != RCL_RET_OK {
        return ret;
    }

    nros_core::Clock::clear_ros_time_override();
    RCL_RET_OK
}

/// Whether the ROS time override is in effect.
///
/// Mirrors `rcl_is_enabled_ros_time_override`. Writes `is_enabled` only on
/// `RCL_RET_OK`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_is_enabled_ros_time_override(
    clock: *const rcl_clock_t,
    is_enabled: *mut bool,
) -> rcl_ret_t {
    if is_enabled.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let ret = ros_time_clock_ok(clock);
    if ret != RCL_RET_OK {
        return ret;
    }

    *is_enabled = nros_core::Clock::is_ros_time_override_active();
    RCL_RET_OK
}

/// Set the time a `RCL_ROS_TIME` clock reads, in nanoseconds since the
/// epoch — the entry point a `/clock` subscriber or a bag player calls.
///
/// Mirrors `rcl_set_ros_time_override`, with the one model difference stated at
/// the top of this section: setting a value also ENABLES the override, and a
/// NEGATIVE `nanoseconds` is `RCL_RET_INVALID_ARGUMENT` because a negative
/// value is the "no override" sentinel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_set_ros_time_override(
    clock: *const rcl_clock_t,
    nanoseconds: i64,
) -> rcl_ret_t {
    if nanoseconds < 0 {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let ret = ros_time_clock_ok(clock);
    if ret != RCL_RET_OK {
        return ret;
    }

    nros_core::Clock::set_ros_time_override(nanoseconds);
    RCL_RET_OK
}

/// Read the ROS time override back, in nanoseconds since the epoch.
///
/// The C face of `nros_core::Clock::get_ros_time_override`, whose `Option` this
/// splits into a status and an out-parameter: `RCL_RET_OK` and a value when an
/// override is in effect, `RCLC_RET_NOT_FOUND` and an untouched out-parameter
/// when none is. rcl has no equivalent — a caller there reads the clock — but
/// dropping it would leave the C mirror of the Rust surface one entry point
/// short, and "is a simulator driving me, and at what time" is one question.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_get_ros_time_override(
    clock: *const rcl_clock_t,
    nanoseconds: *mut i64,
) -> rcl_ret_t {
    if nanoseconds.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }
    let ret = ros_time_clock_ok(clock);
    if ret != RCL_RET_OK {
        return ret;
    }

    match nros_core::Clock::get_ros_time_override() {
        Some(t) => {
            *nanoseconds = t.to_nanos();
            RCL_RET_OK
        }
        None => RCLC_RET_NOT_FOUND,
    }
}

/// Whether the clock has produced a time yet.
///
/// Mirrors `rcl_clock_time_started`, including its implementation: read the
/// clock and report whether the value is non-zero. For a ROS-time clock that is
/// the "enabled, but no `/clock` sample has arrived" state — the one
/// `rcl_enable_ros_time_override` leaves behind — and for a system or steady
/// clock it is true as soon as the clock is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rcl_clock_time_started(clock: *const rcl_clock_t) -> bool {
    let mut nanoseconds: i64 = 0;
    if rcl_clock_get_now(clock, &mut nanoseconds) != RCL_RET_OK {
        return false;
    }
    nanoseconds != 0
}

// ============================================================================
// Time Utility Functions
// ============================================================================

/// Split a signed nanosecond count into the `(sec, nanosec)` pair
/// `builtin_interfaces` carries (issue 0799).
///
/// `Time.msg` states the convention in its own comments: *"The nanoseconds
/// component, valid in the range [0, 1e9), to be added to the seconds
/// component. The time -1.7 seconds is represented as {sec: -2, nanosec:
/// 3e8}"*. That is FLOOR division with a NON-NEGATIVE remainder, which is
/// exactly `div_euclid` / `rem_euclid` — and exactly what truncation plus
/// `unsigned_abs` (what this used to be) is not: `-0.5 s` came out as
/// `{sec: 0, nanosec: 5e8}`, which decodes as `+0.5 s`.
///
/// # Out-of-range input SATURATES, it does not wrap
///
/// `sec` is `int32` on the wire, so the encodable range is roughly ±68 years of
/// nanoseconds while the input is an `i64` reaching ±292. The old `as i32`
/// truncated the high bits, so one nanosecond past the end came back as a time
/// 136 years in the other direction — a silent sign flip, the same failure this
/// function was fixed for. Saturating keeps the encoding MONOTONE (an
/// unrepresentable instant clamps to the nearest representable one and still
/// compares in the right direction) and is the only choice available: the
/// signature returns a value with no error channel, and a `panic!` in a
/// `no_std` C entry point aborts the image over a timestamp.
fn split_nanoseconds(nanoseconds: i64) -> (i32, u32) {
    let sec = nanoseconds.div_euclid(NANOS_PER_SEC as i64);
    if sec > i32::MAX as i64 {
        return (i32::MAX, NANOS_PER_SEC as u32 - 1);
    }
    if sec < i32::MIN as i64 {
        return (i32::MIN, 0);
    }
    // `rem_euclid` with a positive divisor is guaranteed in `[0, 1e9)`, which
    // is the range `Time.msg` requires of `nanosec`.
    (
        sec as i32,
        nanoseconds.rem_euclid(NANOS_PER_SEC as i64) as u32,
    )
}

/// Convert nanoseconds to a rclc_time_t structure.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_time_from_nanoseconds(nanoseconds: i64) -> rclc_time_t {
    let (sec, nanosec) = split_nanoseconds(nanoseconds);
    rclc_time_t { sec, nanosec }
}

/// Convert nanoseconds to a rcl_duration_t structure.
///
/// The duration half of `rclc_time_from_nanoseconds`, and the same encoding —
/// `builtin_interfaces/msg/Duration` has the identical `{sec, nanosec}` shape
/// and the identical `[0, 1e9)` rule on `nanosec`. Issue 0799: a negative span
/// is ordinary ("how far ahead of the deadline are we" is negative half the
/// time), and until this existed the C++ `Duration::to_msg` open-coded the
/// split rather than delegate to a `_time_` entry point that got negatives
/// wrong.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_duration_from_nanoseconds(nanoseconds: i64) -> rcl_duration_t {
    let (sec, nanosec) = split_nanoseconds(nanoseconds);
    rcl_duration_t { sec, nanosec }
}

/// Convert a rclc_time_t structure to nanoseconds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_time_to_nanoseconds(time: *const rclc_time_t) -> i64 {
    if time.is_null() {
        return 0;
    }

    let time = &*time;
    (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64)
}

/// Add a duration to a time.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_time_add(time: rclc_time_t, duration: rcl_duration_t) -> rclc_time_t {
    let time_ns = (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64);
    let duration_ns = (duration.sec as i64) * (NANOS_PER_SEC as i64) + (duration.nanosec as i64);

    rclc_time_from_nanoseconds(time_ns + duration_ns)
}

/// Subtract a duration from a time.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_time_sub(time: rclc_time_t, duration: rcl_duration_t) -> rclc_time_t {
    let time_ns = (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64);
    let duration_ns = (duration.sec as i64) * (NANOS_PER_SEC as i64) + (duration.nanosec as i64);

    rclc_time_from_nanoseconds(time_ns - duration_ns)
}

/// Compare two times.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_time_compare(a: rclc_time_t, b: rclc_time_t) -> c_int {
    if a.sec < b.sec {
        -1
    } else if a.sec > b.sec {
        1
    } else if a.nanosec < b.nanosec {
        -1
    } else if a.nanosec > b.nanosec {
        1
    } else {
        0
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const NS: i64 = NANOS_PER_SEC as i64;

    /// The two surfaces answer "what is ROS time with no `/clock` installed"
    /// identically — issue 1334, and the cross-language negative control.
    ///
    /// This lane links a real platform port (that is why `platform::tests`
    /// passes here at all), so the two answers are separable: C reads the
    /// port's wall clock and Rust used to read an in-image counter nothing
    /// advances, which on the pre-fix tree makes the assertion below fail by
    /// roughly the age of the Unix epoch.
    ///
    /// The tolerance is a real sample of a running clock on each side, so it
    /// cannot be zero; a second is three orders of magnitude above the gap
    /// between two `clock_gettime` calls and nine below the defect.
    #[test]
    fn ros_time_agrees_with_the_rust_surface() {
        nros_core::clock::Clock::clear_ros_time_override();

        let c_side = get_ros_time_ns();
        let rust_side = nros_core::clock::Clock::ros_time().now().to_nanos();

        assert!(
            (c_side - rust_side).abs() < NS,
            "the C and Rust ROS-time surfaces must read the same clock: \
             C said {c_side} ns, Rust said {rust_side} ns"
        );
        assert!(
            rust_side > 1_600_000_000 * NS,
            "and it must be the WALL clock (after 2020), not a counter: {rust_side} ns"
        );
    }

    /// The same agreement for the other two clock types, which is what makes
    /// the one above a statement about ROS time rather than about this image's
    /// clocks in general — issue 1334's sweep.
    #[test]
    fn every_clock_type_agrees_with_the_rust_surface() {
        let wall_gap = get_system_time_ns() - nros_core::clock::Clock::system().now().to_nanos();
        assert!(
            wall_gap.abs() < NS,
            "system time disagrees by {wall_gap} ns"
        );

        let steady_gap =
            get_steady_time_ns() as i64 - nros_core::clock::Clock::steady().now().to_nanos();
        assert!(
            steady_gap.abs() < NS,
            "steady time disagrees by {steady_gap} ns"
        );
    }

    /// The wire decoder, which is the correct inverse of the correct encoding —
    /// so a round trip through it EXPOSES an encoder error rather than
    /// cancelling it (issue 0799).
    fn decode(t: rclc_time_t) -> i64 {
        unsafe { rclc_time_to_nanoseconds(&t) }
    }

    /// The largest and smallest nanosecond counts an `int32` seconds field can
    /// hold. Beyond these `split_nanoseconds` saturates by design.
    const MAX_ENCODABLE_NS: i64 = (i32::MAX as i64) * NS + (NS - 1);
    const MIN_ENCODABLE_NS: i64 = (i32::MIN as i64) * NS;

    /// `builtin_interfaces/msg/Time.msg` states the convention in its own
    /// comments; these are the two values it names, plus their mirror images.
    #[test]
    fn encodes_the_convention_time_msg_documents() {
        // "The time -1.7 seconds is represented as {sec: -2, nanosec: 3e8}"
        assert_eq!(
            rclc_time_from_nanoseconds(-1_700_000_000),
            rclc_time_t {
                sec: -2,
                nanosec: 300_000_000
            }
        );
        // The sub-second case that used to come back with the sign gone.
        assert_eq!(
            rclc_time_from_nanoseconds(-500_000_000),
            rclc_time_t {
                sec: -1,
                nanosec: 500_000_000
            }
        );
        assert_eq!(
            rclc_time_from_nanoseconds(1_700_000_000),
            rclc_time_t {
                sec: 1,
                nanosec: 700_000_000
            }
        );
        // Exactly on a second boundary: no borrow, `nanosec` is zero on both
        // sides of zero.
        assert_eq!(
            rclc_time_from_nanoseconds(-2 * NS),
            rclc_time_t {
                sec: -2,
                nanosec: 0
            }
        );
        assert_eq!(rclc_time_from_nanoseconds(0), rclc_time_t::default());
    }

    /// The deliverable of issue 0799: encode/decode is the identity over
    /// NEGATIVE, zero, sub-second and multi-second values, and every encoding
    /// obeys `nanosec ∈ [0, 1e9)`.
    ///
    /// The old encoder (truncate toward zero, `unsigned_abs` remainder) fails
    /// this on the first negative sub-second value — verified by reverting it
    /// and re-running, which is exactly the check the positives-only tests
    /// could not make.
    #[test]
    fn round_trips_over_negative_zero_subsecond_and_multisecond() {
        // Hand-picked coordinates first: the boundaries a generator hits only
        // by luck.
        let edges: [i64; 24] = [
            0,
            1,
            -1,
            999_999_999,
            -999_999_999,
            NS,
            -NS,
            NS + 1,
            -NS - 1,
            NS - 1,
            -(NS - 1),
            -500_000_000,
            -1_700_000_000,
            500_000_000,
            1_700_000_000,
            -86_400 * NS,
            86_400 * NS,
            -86_400 * NS - 1,
            123 * NS + 456,
            -123 * NS - 456,
            MAX_ENCODABLE_NS,
            MIN_ENCODABLE_NS,
            MAX_ENCODABLE_NS - 1,
            MIN_ENCODABLE_NS + 1,
        ];

        // Then a sweep. A deterministic LCG rather than a random source: a
        // property test that cannot be re-run on the value that failed is a
        // report nobody can act on. The magnitude cycles through nanoseconds,
        // milliseconds, seconds and days so sub-second and multi-second values
        // are both dense in the sample, and each is tried with both signs.
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let scales = [1i64, 1_000, 1_000_000, NS, 3_600 * NS];
        let generated = (0..20_000).map(|i| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let scale = scales[i % scales.len()];
            let magnitude = ((state >> 11) as i64).rem_euclid(MAX_ENCODABLE_NS / scale) * scale;
            if i % 2 == 0 { magnitude } else { -magnitude }
        });

        for ns in edges.into_iter().chain(generated) {
            let t = rclc_time_from_nanoseconds(ns);

            assert!(
                t.nanosec < NS as u32,
                "nanosec out of the [0, 1e9) range Time.msg requires: {ns} -> {t:?}"
            );
            assert_eq!(decode(t), ns, "round trip lost {ns}: encoded as {t:?}");
            // Floor, not truncation: the seconds field never rounds toward zero.
            assert_eq!(
                t.sec as i64,
                ns.div_euclid(NS),
                "seconds did not floor for {ns}"
            );
            // A negative instant must decode negative. Stated separately from
            // the round trip because THIS is the user-visible symptom: -0.5 s
            // used to come back as +0.5 s.
            assert_eq!(
                decode(t).signum(),
                ns.signum(),
                "sign lost encoding {ns} as {t:?}"
            );

            // The duration encoding is the same encoding.
            let d = rclc_duration_from_nanoseconds(ns);
            assert_eq!(
                (d.sec, d.nanosec),
                (t.sec, t.nanosec),
                "duration != time for {ns}"
            );
        }
    }

    /// Past ±68 years of nanoseconds an `int32` seconds field cannot hold the
    /// value. The old `as i32` wrapped, turning an overflow into a time in the
    /// opposite direction; this clamps to the nearest representable instant, so
    /// the encoding stays monotone and the sign survives.
    #[test]
    fn out_of_range_saturates_rather_than_wrapping() {
        let past_max = rclc_time_from_nanoseconds(MAX_ENCODABLE_NS + 1);
        assert_eq!(
            past_max,
            rclc_time_t {
                sec: i32::MAX,
                nanosec: 999_999_999
            }
        );
        assert_eq!(rclc_time_from_nanoseconds(i64::MAX), past_max);

        let past_min = rclc_time_from_nanoseconds(MIN_ENCODABLE_NS - 1);
        assert_eq!(
            past_min,
            rclc_time_t {
                sec: i32::MIN,
                nanosec: 0
            }
        );
        assert_eq!(rclc_time_from_nanoseconds(i64::MIN), past_min);

        // Monotone across the clamp, and still signed the right way — the two
        // properties wrapping destroyed.
        assert!(decode(past_max) > 0);
        assert!(decode(past_min) < 0);
        assert!(decode(past_max) > decode(rclc_time_from_nanoseconds(MAX_ENCODABLE_NS - NS)));
        assert!(decode(past_min) < decode(rclc_time_from_nanoseconds(MIN_ENCODABLE_NS + NS)));
    }

    /// `rclc_time_add` / `rclc_time_sub` route their `i64` result through the
    /// encoder, so a result landing in (−1 s, 0) is exactly the case that used
    /// to come back positive.
    #[test]
    fn arithmetic_across_zero_keeps_its_sign() {
        let quarter_second = rcl_duration_t {
            sec: 0,
            nanosec: 250_000_000,
        };

        // 0.1 s − 0.25 s = −0.15 s, which lands inside the broken window.
        let t = rclc_time_t {
            sec: 0,
            nanosec: 100_000_000,
        };
        let diff = rclc_time_sub(t, quarter_second);
        assert_eq!(decode(diff), -150_000_000);
        assert_eq!(
            diff,
            rclc_time_t {
                sec: -1,
                nanosec: 850_000_000
            }
        );

        // Adding it back is the identity.
        assert_eq!(rclc_time_add(diff, quarter_second), t);

        // And ordering survives: an earlier time compares less, which is what
        // a receiver reasoning about latency depends on.
        assert_eq!(rclc_time_compare(diff, t), -1);
    }

    /// Issue 0789 — the ROS-time switches C did not have, and the clock read
    /// they drive. One test rather than five: the override is process-global
    /// (as it is in Rust), so separate `#[test]`s would race under nextest's
    /// thread pool.
    #[test]
    fn ros_time_override_drives_a_ros_clock() {
        let mut ros = rclc_clock_get_zero_initialized();
        let mut system = rclc_clock_get_zero_initialized();
        unsafe {
            assert_eq!(
                rcl_clock_init(
                    rcl_clock_type_t::RCL_ROS_TIME,
                    &mut ros,
                    core::ptr::null_mut()
                ),
                RCL_RET_OK
            );
            assert_eq!(
                rcl_clock_init(
                    rcl_clock_type_t::RCL_SYSTEM_TIME,
                    &mut system,
                    core::ptr::null_mut()
                ),
                RCL_RET_OK
            );

            // Start from a known state — another crate in the image may have
            // installed one.
            assert_eq!(rcl_disable_ros_time_override(&ros), RCL_RET_OK);

            let mut enabled = true;
            assert_eq!(
                rcl_is_enabled_ros_time_override(&ros, &mut enabled),
                RCL_RET_OK
            );
            assert!(!enabled);

            let mut nanoseconds: i64 = 42;
            assert_eq!(
                rclc_get_ros_time_override(&ros, &mut nanoseconds),
                RCLC_RET_NOT_FOUND
            );
            assert_eq!(nanoseconds, 42, "out-parameter written with no override");

            // With no override the ROS clock reads the wall, as it always has.
            assert_eq!(rcl_clock_get_now(&ros, &mut nanoseconds), RCL_RET_OK);
            assert!(nanoseconds > 0);
            assert!(rcl_clock_time_started(&ros));

            // Enabled but no `/clock` sample yet: time 0, and NOT started —
            // the state `rcl_clock_time_started` exists to report.
            assert_eq!(rcl_enable_ros_time_override(&ros), RCL_RET_OK);
            assert_eq!(
                rcl_is_enabled_ros_time_override(&ros, &mut enabled),
                RCL_RET_OK
            );
            assert!(enabled);
            assert_eq!(rcl_clock_get_now(&ros, &mut nanoseconds), RCL_RET_OK);
            assert_eq!(nanoseconds, 0);
            assert!(!rcl_clock_time_started(&ros));

            // A `/clock` sample arrives.
            let simulated = 1_234_500_000_000i64;
            assert_eq!(rcl_set_ros_time_override(&ros, simulated), RCL_RET_OK);
            assert_eq!(rcl_clock_get_now(&ros, &mut nanoseconds), RCL_RET_OK);
            assert_eq!(nanoseconds, simulated);
            assert!(rcl_clock_time_started(&ros));

            let mut read_back: i64 = 0;
            assert_eq!(rclc_get_ros_time_override(&ros, &mut read_back), RCL_RET_OK);
            assert_eq!(read_back, simulated);

            let mut stamp = rclc_time_t::default();
            assert_eq!(rclc_clock_get_now_time(&ros, &mut stamp), RCL_RET_OK);
            assert_eq!(
                stamp,
                rclc_time_t {
                    sec: 1234,
                    nanosec: 500_000_000
                }
            );

            // Time does not move while the simulator holds it.
            assert_eq!(rcl_clock_get_now(&ros, &mut nanoseconds), RCL_RET_OK);
            assert_eq!(nanoseconds, simulated);

            // A negative override is the "no override" sentinel, so it is
            // refused rather than silently clearing the clock.
            assert_eq!(
                rcl_set_ros_time_override(&ros, -1),
                RCL_RET_INVALID_ARGUMENT
            );
            assert_eq!(rclc_get_ros_time_override(&ros, &mut read_back), RCL_RET_OK);
            assert_eq!(read_back, simulated);

            // A system clock is not something `/clock` can drive, and the
            // system clock keeps reading the wall while ROS time is held.
            assert_eq!(rcl_enable_ros_time_override(&system), RCLC_RET_NOT_ALLOWED);
            assert_eq!(
                rcl_is_enabled_ros_time_override(&system, &mut enabled),
                RCLC_RET_NOT_ALLOWED
            );
            assert_eq!(rcl_clock_get_now(&system, &mut nanoseconds), RCL_RET_OK);
            assert!(nanoseconds > simulated);

            // Null clock, null out-parameter, and an uninitialised clock.
            assert_eq!(
                rcl_enable_ros_time_override(core::ptr::null()),
                RCL_RET_INVALID_ARGUMENT
            );
            assert_eq!(
                rcl_is_enabled_ros_time_override(&ros, core::ptr::null_mut()),
                RCL_RET_INVALID_ARGUMENT
            );
            let fresh = rclc_clock_get_zero_initialized();
            assert_eq!(rcl_enable_ros_time_override(&fresh), RCL_RET_NOT_INIT);
            assert!(!rcl_clock_time_started(&fresh));

            // Released: the ROS clock reads the wall again.
            assert_eq!(rcl_disable_ros_time_override(&ros), RCL_RET_OK);
            assert_eq!(
                rcl_is_enabled_ros_time_override(&ros, &mut enabled),
                RCL_RET_OK
            );
            assert!(!enabled);
            assert_eq!(rcl_clock_get_now(&ros, &mut nanoseconds), RCL_RET_OK);
            assert!(nanoseconds > simulated);
        }
    }

    /// issue 1975 -- `nros_timer_clock_type` is the inverse of
    /// `nros_timer_clock_source` for every source, so a clock read back off a
    /// timer is the one it was created with.
    #[cfg(feature = "rmw-cffi")]
    #[test]
    fn timer_clock_type_inverts_timer_clock_source() {
        use nros_node::executor::TimerClockSource;
        for source in [
            TimerClockSource::Steady,
            TimerClockSource::Ros,
            TimerClockSource::System,
        ] {
            assert_eq!(
                nros_timer_clock_source(nros_timer_clock_type(source)),
                Some(source)
            );
        }
        assert_eq!(
            nros_timer_clock_type(TimerClockSource::Steady),
            rcl_clock_type_t::RCL_STEADY_TIME as u8
        );
    }
}
