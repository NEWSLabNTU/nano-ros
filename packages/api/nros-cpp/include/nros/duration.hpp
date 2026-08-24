// nros-cpp: Duration — a signed span of time
// Freestanding C++ — no exceptions, no STL required

/**
 * @file duration.hpp
 * @ingroup grp_clock
 * @brief `nros::Duration` — a signed nanosecond span, mirroring `rclcpp::Duration`.
 *
 * Issue 0789. The value surface already existed in C (`nros_duration_t`,
 * `nros_time_add`, `nros_time_sub`) and in Rust (`Duration`, `TimerDuration`);
 * this is the C++ face over it, so a ported rclcpp node that builds a duration
 * or stamps a header compiles here.
 *
 * Two members of `rclcpp::Duration` are deliberately absent, both already
 * recorded in `docs/reference/api-parity-ledger/timer.json`:
 *   * `to_chrono` — `<chrono>` is not available on the freestanding targets.
 *   * `from_rmw_time` / `to_rmw_time` — the RMW seam is not a user-facing type
 *     here (RFC-0054); a user writes nanoseconds or milliseconds.
 */

#ifndef NROS_CPP_DURATION_HPP
#define NROS_CPP_DURATION_HPP

#include <cstdint>
// Freestanding C++ (`-ffreestanding`) is only required to put the fixed-width
// types and their limit macros in the GLOBAL namespace via `<stdint.h>`;
// `INT32_MAX` below is spelled unqualified for that reason.
#include <stdint.h>

namespace nros {

/// Nanoseconds in one second. Named once here; `time.hpp` reuses it.
constexpr int64_t NANOSECONDS_PER_SECOND = 1000000000LL;

namespace detail {

/// Split a signed nanosecond count into the `(sec, nanosec)` pair a
/// `builtin_interfaces` message carries.
///
/// `nanosec` is always in `[0, 1e9)`, so the seconds field FLOORS rather than
/// truncating toward zero. That matters only for negative spans, which is why
/// the C `nros_time_from_nanoseconds` (which truncates, and so renders
/// `-0.5 s` as `sec = 0, nanosec = 500000000` — i.e. `+0.5 s`) is not reused
/// for a duration. A timestamp is non-negative and keeps using the C entry
/// point; see `Time::to_msg`.
inline void split_nanoseconds(int64_t ns, int32_t& sec, uint32_t& nanosec) {
    int64_t s = ns / NANOSECONDS_PER_SECOND;
    int64_t rem = ns % NANOSECONDS_PER_SECOND;
    if (rem < 0) {
        s -= 1;
        rem += NANOSECONDS_PER_SECOND;
    }
    sec = static_cast<int32_t>(s);
    nanosec = static_cast<uint32_t>(rem);
}

} // namespace detail

/// A signed span of time, held as nanoseconds.
///
/// Mirrors `rclcpp::Duration`'s user-facing surface. Every accessor and every
/// operator is `constexpr`, so a period or a deadline can be computed at
/// compile time and baked into an image.
///
/// Usage:
/// ```cpp
/// auto period = nros::Duration::from_seconds(0.1);
/// auto budget = nros::Duration::from_nanoseconds(500000);
/// if (elapsed > period) { /* overrun */ }
/// ```
class Duration {
  public:
    /// Zero. (`rclcpp::Duration` has no default constructor; a freestanding
    /// value type that can live in a static array needs one.)
    constexpr Duration() : ns_(0) {}

    /// Seconds + nanoseconds, the `rclcpp::Duration(int32_t, uint32_t)` shape.
    constexpr Duration(int32_t seconds, uint32_t nanoseconds)
        : ns_(static_cast<int64_t>(seconds) * NANOSECONDS_PER_SECOND +
              static_cast<int64_t>(nanoseconds)) {}

    /// Build from a raw signed nanosecond count.
    static constexpr Duration from_nanoseconds(int64_t nanoseconds) {
        return Duration(RawNanoseconds{nanoseconds});
    }

    /// Build from fractional seconds. Truncates toward zero, as rclcpp does.
    static constexpr Duration from_seconds(double seconds) {
        return Duration(RawNanoseconds{
            static_cast<int64_t>(seconds * static_cast<double>(NANOSECONDS_PER_SECOND))});
    }

    /// The largest representable duration — `rclcpp::Duration::max()`'s value.
    static constexpr Duration max() { return Duration(INT32_MAX, 999999999u); }

    /// The span in nanoseconds.
    constexpr int64_t nanoseconds() const { return ns_; }

    /// The span in (fractional) seconds.
    constexpr double seconds() const {
        return static_cast<double>(ns_) / static_cast<double>(NANOSECONDS_PER_SECOND);
    }

    // -- Arithmetic -------------------------------------------------------
    //
    // rclcpp raises `std::overflow_error` on wraparound. `-fno-exceptions`
    // (RFC-0018) leaves no way to report that from an operator, so these
    // saturate nothing and wrap like the underlying `int64_t`: at ~292 years
    // of nanoseconds the bound is not one an embedded node reaches.

    constexpr Duration operator+(const Duration& rhs) const {
        return Duration(RawNanoseconds{ns_ + rhs.ns_});
    }
    constexpr Duration operator-(const Duration& rhs) const {
        return Duration(RawNanoseconds{ns_ - rhs.ns_});
    }
    constexpr Duration operator-() const { return Duration(RawNanoseconds{-ns_}); }
    constexpr Duration operator*(double scale) const {
        return Duration(RawNanoseconds{static_cast<int64_t>(static_cast<double>(ns_) * scale)});
    }

    Duration& operator+=(const Duration& rhs) {
        ns_ += rhs.ns_;
        return *this;
    }
    Duration& operator-=(const Duration& rhs) {
        ns_ -= rhs.ns_;
        return *this;
    }

    // -- Comparison -------------------------------------------------------

    constexpr bool operator==(const Duration& rhs) const { return ns_ == rhs.ns_; }
    constexpr bool operator!=(const Duration& rhs) const { return ns_ != rhs.ns_; }
    constexpr bool operator<(const Duration& rhs) const { return ns_ < rhs.ns_; }
    constexpr bool operator<=(const Duration& rhs) const { return ns_ <= rhs.ns_; }
    constexpr bool operator>(const Duration& rhs) const { return ns_ > rhs.ns_; }
    constexpr bool operator>=(const Duration& rhs) const { return ns_ >= rhs.ns_; }

    /// Fill a generated `builtin_interfaces/msg/Duration` — anything with
    /// `sec` / `nanosec` members, which is the shape rosidl-codegen emits.
    ///
    /// rclcpp spells this as an implicit `operator builtin_interfaces::msg::
    /// Duration()`. nros-cpp is header-only and message types are generated
    /// per user package, so the client library cannot name one; a template
    /// that binds to the generated struct is the same conversion without the
    /// dependency.
    ///
    /// ```cpp
    /// nros::Duration::from_seconds(0.1).to_msg(msg.lifespan);
    /// ```
    template <typename DurationMsgT> void to_msg(DurationMsgT& out) const {
        int32_t sec = 0;
        uint32_t nanosec = 0;
        detail::split_nanoseconds(ns_, sec, nanosec);
        out.sec = sec;
        out.nanosec = nanosec;
    }

  private:
    /// Tag type so the raw-nanosecond constructor cannot be confused with the
    /// `(seconds, nanoseconds)` one. `from_nanoseconds` is the public spelling.
    struct RawNanoseconds {
        int64_t value;
    };
    explicit constexpr Duration(RawNanoseconds raw) : ns_(raw.value) {}

    int64_t ns_;
};

} // namespace nros

#endif // NROS_CPP_DURATION_HPP
