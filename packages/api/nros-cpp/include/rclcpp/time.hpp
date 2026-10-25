// nros-cpp: Time — a point on a clock's timeline
// Freestanding C++ — no exceptions, no STL required

/**
 * @file time.hpp
 * @ingroup grp_clock
 * @brief `rclcpp::Time` — a timestamp, mirroring upstream's `rclcpp::Time`.
 *
 * Issue 0789. `rclc_time_t` and the `nros_time_*` arithmetic have existed in C
 * since the C API shipped; this is the C++ face over them, so
 * `msg.header.stamp` can be filled from `node.now()` instead of by hand.
 */

#ifndef NROS_RCLCPP_TIME_HPP
#define NROS_RCLCPP_TIME_HPP

#include <cstdint>
#include <stdint.h>

#include "rclcpp/duration.hpp"

// `rcl_clock_type_t` and `rclc_time_from_nanoseconds` — the C surface this
// wraps. `nros_generated.h` carries its own `extern "C"` guard.
#include "rclc/types.h"

// ============================================================================
// `rclcpp::Time` — DEFINED here (RFC-0089: rclcpp:: is the home)
// ============================================================================
//
// phase-428: the definition moved from `nros::` to `rclcpp::` (the `nros::`
// alias went in phase-483 W1), so a ported `rclcpp::Time stamp = node->now();`
// and in-tree code name the same type.
namespace rclcpp {

/// A point in time, held as nanoseconds on a named clock.
///
/// Mirrors upstream's `rclcpp::Time`. The clock TYPE travels with the value, as it does in
/// rclcpp, so a steady-clock reading and a system-clock reading are
/// distinguishable after the fact.
///
/// Divergence from rclcpp, forced by `-fno-exceptions` (RFC-0018): rclcpp
/// throws `std::runtime_error` when two `Time`s of different clock types are
/// compared or subtracted. There is no way to report that from an operator
/// here, so the comparison is on nanoseconds alone. Check `get_clock_type()`
/// when the distinction matters.
///
/// Usage:
/// ```cpp
/// rclcpp::Time stamp = node.now();
/// stamp.to_msg(msg.header.stamp);
/// rclcpp::Duration age = node.now() - stamp;
/// ```
class Time {
  public:
    /// Nanoseconds since the clock's epoch. Both arguments default, so
    /// `Time()` is the zero point on the system clock — the
    /// `rclcpp::Time(int64_t = 0, rcl_clock_type_t = RCL_SYSTEM_TIME)` shape.
    constexpr Time(int64_t nanoseconds = 0, rcl_clock_type_t clock_type = RCL_SYSTEM_TIME)
        : ns_(nanoseconds), clock_type_(clock_type) {}

    /// Seconds + nanoseconds, the `rclcpp::Time(int32_t, uint32_t,
    /// rcl_clock_type_t)` shape.
    constexpr Time(int32_t seconds, uint32_t nanoseconds,
                   rcl_clock_type_t clock_type = RCL_SYSTEM_TIME)
        : ns_(static_cast<int64_t>(seconds) * ::rclcpp::NANOSECONDS_PER_SECOND +
              static_cast<int64_t>(nanoseconds)),
          clock_type_(clock_type) {}

    /// From a generated `builtin_interfaces/msg/Time` — anything with `sec` and
    /// `nanosec` members, the shape `to_msg` below binds to (phase-488).
    /// Upstream's `Time(const builtin_interfaces::msg::Time&, rcl_clock_type_t
    /// = RCL_ROS_TIME)` is implicit, and ported code leans on that:
    /// `rclcpp::Time(msg->header.stamp)` appears in 78 of the 2944 sources of
    /// an Autoware 0.45.1 workspace, and Nav2's `nav2_util` writes it too.
    /// A template rather than the message type, because message types are
    /// generated per user package and this header cannot name one.
    template <typename TimeMsgT, typename = decltype(static_cast<int64_t>(TimeMsgT().sec) +
                                                     static_cast<int64_t>(TimeMsgT().nanosec))>
    // NOLINTNEXTLINE(google-explicit-constructor)
    constexpr Time(const TimeMsgT& msg, rcl_clock_type_t clock_type = RCL_ROS_TIME)
        : ns_(static_cast<int64_t>(msg.sec) * ::rclcpp::NANOSECONDS_PER_SECOND +
              static_cast<int64_t>(msg.nanosec)),
          clock_type_(clock_type) {}

    /// The largest representable time — `rclcpp::Time::max()`'s value.
    static constexpr Time max() { return Time(INT32_MAX, 999999999u); }

    /// Nanoseconds since the clock's epoch.
    constexpr int64_t nanoseconds() const { return ns_; }

    /// (Fractional) seconds since the clock's epoch.
    constexpr double seconds() const {
        return static_cast<double>(ns_) / static_cast<double>(::rclcpp::NANOSECONDS_PER_SECOND);
    }

    /// Which clock this value was read from.
    constexpr rcl_clock_type_t get_clock_type() const { return clock_type_; }

    // -- Arithmetic -------------------------------------------------------

    constexpr Time operator+(const Duration& rhs) const {
        return Time(ns_ + rhs.nanoseconds(), clock_type_);
    }
    constexpr Time operator-(const Duration& rhs) const {
        return Time(ns_ - rhs.nanoseconds(), clock_type_);
    }
    /// The span between two instants. See the clock-type note above.
    constexpr Duration operator-(const Time& rhs) const {
        return Duration::from_nanoseconds(ns_ - rhs.ns_);
    }

    Time& operator+=(const Duration& rhs) {
        ns_ += rhs.nanoseconds();
        return *this;
    }
    Time& operator-=(const Duration& rhs) {
        ns_ -= rhs.nanoseconds();
        return *this;
    }

    // -- Comparison -------------------------------------------------------

    constexpr bool operator==(const Time& rhs) const { return ns_ == rhs.ns_; }
    constexpr bool operator!=(const Time& rhs) const { return ns_ != rhs.ns_; }
    constexpr bool operator<(const Time& rhs) const { return ns_ < rhs.ns_; }
    constexpr bool operator<=(const Time& rhs) const { return ns_ <= rhs.ns_; }
    constexpr bool operator>(const Time& rhs) const { return ns_ > rhs.ns_; }
    constexpr bool operator>=(const Time& rhs) const { return ns_ >= rhs.ns_; }

    /// Fill a generated `builtin_interfaces/msg/Time` — anything with `sec` /
    /// `nanosec` members, which is the shape rosidl-codegen emits. This is the
    /// call a publisher actually needs: stamping a header.
    ///
    /// rclcpp spells it as an implicit `operator builtin_interfaces::msg::
    /// Time()`. nros-cpp is header-only and message types are generated per
    /// user package, so the client library cannot name one; a template that
    /// binds to the generated struct is the same conversion without the
    /// dependency. (`rust:Time::to_ros_msg` is the same call one language over.
    /// Rust has no structural bound, so phase-467 Q3 shipped it as an AUTHORED
    /// trait `nros_core::SecNanosecMsg` that codegen implements for the same
    /// `{ sec, nanosec }` shape this template binds to.)
    ///
    /// ```cpp
    /// node.now().to_msg(msg.header.stamp);
    /// ```
    template <typename TimeMsgT> void to_msg(TimeMsgT& out) const {
        // The C decomposition floors and keeps `nanosec` in `[0, 1e9)`, which
        // is what `Time.msg` specifies — for a time before the epoch as much
        // as after it (issue 0799). `Duration::to_msg` is the same call on the
        // duration entry point.
        rclc_time_t t = rclc_time_from_nanoseconds(ns_);
        out.sec = t.sec;
        out.nanosec = t.nanosec;
    }

    /// Upstream's implicit `operator builtin_interfaces::msg::Time()`
    /// (phase-488; Nav2 writes `msg.header.stamp = now();`). The same
    /// `{sec, nanosec}` template as the constructor above, through `to_msg`.
    template <typename TimeMsgT, typename = decltype(static_cast<int64_t>(TimeMsgT().sec) +
                                                     static_cast<int64_t>(TimeMsgT().nanosec))>
    // NOLINTNEXTLINE(google-explicit-constructor)
    operator TimeMsgT() const {
        TimeMsgT out{};
        to_msg(out);
        return out;
    }

  private:
    int64_t ns_;
    rcl_clock_type_t clock_type_;
};

/// `duration + time`, so the sum reads either way round. The one free operator
/// `rclcpp` declares for these types; every other one is a member on both
/// sides.
constexpr Time operator+(const Duration& lhs, const Time& rhs) {
    return rhs + lhs;
}

/// Upstream's `builtin_interfaces::msg::Time operator+(const
/// builtin_interfaces::msg::Time&, const rclcpp::Duration&)` and its `-`
/// (phase-488; Nav2 writes `stamp + delta`). Over any `{sec, nanosec}`
/// message, found by argument-dependent lookup through `Duration`.
template <typename TimeMsgT, typename = decltype(static_cast<int64_t>(TimeMsgT().sec) +
                                                 static_cast<int64_t>(TimeMsgT().nanosec))>
inline TimeMsgT operator+(const TimeMsgT& lhs, const Duration& rhs) {
    TimeMsgT out{};
    (Time(lhs) + rhs).to_msg(out);
    return out;
}
/// @see operator+(const TimeMsgT&, const Duration&)
template <typename TimeMsgT, typename = decltype(static_cast<int64_t>(TimeMsgT().sec) +
                                                 static_cast<int64_t>(TimeMsgT().nanosec))>
inline TimeMsgT operator-(const TimeMsgT& lhs, const Duration& rhs) {
    TimeMsgT out{};
    (Time(lhs) - rhs).to_msg(out);
    return out;
}

} // namespace rclcpp

#endif // NROS_RCLCPP_TIME_HPP
