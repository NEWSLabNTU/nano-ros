// nros-cpp: Clock — a time source
// Freestanding C++ — no exceptions, no STL required

/**
 * @file clock.hpp
 * @ingroup grp_clock
 * @brief `nros::Clock` — reads the current time, mirroring `rclcpp::Clock`.
 *
 * Issue 0789. A thin C++ face over the `nros_clock_*` C surface (RFC-0073
 * defines the platform clock contract underneath it); this header invents no
 * capability of its own.
 */

#ifndef NROS_CPP_CLOCK_HPP
#define NROS_CPP_CLOCK_HPP

#include <cstdint>
#include <stdint.h>

#include "nros/time.hpp"

// The `nros_clock_*` entry points. `nros_generated.h` carries its own
// `extern "C"` guard.
#include "nros/clock.h"

namespace nros {

/// A time source: system, steady, or ROS time.
///
/// Mirrors `rclcpp::Clock`. The clock type is fixed at construction and
/// readable afterwards with `get_clock_type()`.
///
/// Usage:
/// ```cpp
/// nros::Clock steady(NROS_CLOCK_STEADY_TIME);
/// nros::Time t0 = steady.now();
/// // ...
/// nros::Duration elapsed = steady.now() - t0;
/// ```
///
/// Copyable and trivially destructible on purpose. `nros_clock_t` owns no
/// allocation and no handle — `nros_clock_fini` only marks the struct shut
/// down — so unlike `rclcpp::Clock` (which finalises an `rcl_clock_t` holding
/// an allocator) there is nothing for a destructor to release.
///
/// ROS time is only half-present, and the half that is missing is C's: our Rust
/// `Clock` can be driven by a simulator's `/clock`
/// (`set_ros_time_override` and friends) and the C surface this wraps has none
/// of those switches, so `NROS_CLOCK_ROS_TIME` currently reads the system
/// clock. Tracked as `c:enable_ros_time_override` in the parity ledger and in
/// issue 0789; adding the switches to C is what closes it for C++ too.
class Clock {
  public:
    /// Construct a clock of the given type.
    ///
    /// Defaults to `NROS_CLOCK_SYSTEM_TIME`, matching
    /// `rclcpp::Clock(rcl_clock_type_t = RCL_SYSTEM_TIME)`. (A `Node`'s own
    /// clock is ROS time, also as in rclcpp — see `Node::get_clock()`.)
    ///
    /// The clock type is the C enum `nros_clock_type_t` rather than a second
    /// C++ spelling of the same four values: rclcpp likewise takes rcl's
    /// `rcl_clock_type_t`, and one vocabulary across the two languages is the
    /// point of issue 0789.
    explicit Clock(nros_clock_type_t clock_type = NROS_CLOCK_SYSTEM_TIME)
        : clock_(nros_clock_get_zero_initialized()) {
        nros_clock_init(&clock_, clock_type);
    }

    /// The current time.
    ///
    /// Infallible, following the C surface: `nros_clock_get_now_ns` cannot fail
    /// once the clock is valid, because the platform seam (RFC-0073) either has
    /// a clock or the image did not boot. An INVALID clock — one built with
    /// `NROS_CLOCK_UNINITIALIZED` — reads zero; `is_valid()` is how that is
    /// detected, not a `Result` on every timestamp.
    Time now() const {
        int64_t nanoseconds = 0;
        if (nros_clock_get_now_ns(&clock_, &nanoseconds) != 0) {
            nanoseconds = 0;
        }
        return Time(nanoseconds, get_clock_type());
    }

    /// Which kind of clock this is.
    nros_clock_type_t get_clock_type() const { return nros_clock_get_type(&clock_); }

    /// Whether the clock initialised. False for a clock built with an invalid
    /// type; the same predicate as `Node::is_valid()` / `Timer::is_valid()`.
    bool is_valid() const { return nros_clock_is_valid(&clock_); }

  private:
    nros_clock_t clock_;
};

} // namespace nros

#endif // NROS_CPP_CLOCK_HPP
