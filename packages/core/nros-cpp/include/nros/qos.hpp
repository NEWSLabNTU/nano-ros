// nros-cpp: QoS (Quality of Service) profiles
// Freestanding C++ — no STL required

/**
 * @file qos.hpp
 * @ingroup grp_qos
 * @brief `nros::QoS` — full DDS-shaped QoS settings (Phase 108.B.7).
 */

#ifndef NROS_CPP_QOS_HPP
#define NROS_CPP_QOS_HPP

#include <cstdint>

// Forward-declare the FFI types so we don't need to include the generated header
// in user-facing code. The implementation maps these enums to the FFI equivalents.
extern "C" {
struct nros_cpp_qos_t;
}

namespace nros {

/// QoS profile for publishers and subscriptions.
///
/// Mirrors rclcpp::QoS with chainable setters and predefined profiles.
/// All methods are constexpr — profiles can be computed at compile time.
///
/// Phase 108.B.7 — extended with full DDS QoS surface (deadline,
/// lifespan, liveliness kind + lease, avoid_ros_namespace_conventions).
/// Backends advertise per-policy support; entities created with a
/// profile the active backend can't honour return
/// `NROS_CPP_RET_INCOMPATIBLE_QOS` synchronously at create time.
class QoS {
  public:
    /// Liveliness policy kind. Matches DDS `LIVELINESS_QOS_POLICY`.
    enum Liveliness {
        LivelinessNone = 0,
        LivelinessAutomatic = 1,
        LivelinessManualByTopic = 2,
        LivelinessManualByNode = 3,
    };

    /// Default QoS: reliable, volatile, keep-last(10), automatic
    /// liveliness, no deadline / lifespan / lease.
    constexpr QoS()
        : reliability_(Reliable), durability_(Volatile), history_(KeepLast),
          liveliness_(LivelinessAutomatic), depth_(10), deadline_ms_(0), lifespan_ms_(0),
          liveliness_lease_ms_(0), avoid_ros_namespace_conventions_(0) {}

    // -- Chainable setters (match rclcpp fluent API) --

    /// Set reliability to `RELIABLE` (acked transport, retransmits on loss).
    constexpr QoS& reliable() {
        reliability_ = Reliable;
        return *this;
    }

    /// Set reliability to `BEST_EFFORT` (fire-and-forget; default for sensors).
    constexpr QoS& best_effort() {
        reliability_ = BestEffort;
        return *this;
    }

    /// Set durability to `TRANSIENT_LOCAL` — late joiners get the last value.
    constexpr QoS& transient_local() {
        durability_ = TransientLocal;
        return *this;
    }

    /// Set durability to `VOLATILE` — late joiners get nothing (default).
    constexpr QoS& durability_volatile() {
        durability_ = Volatile;
        return *this;
    }

    /// Use `KEEP_LAST` history with the given depth.
    /// @param depth maximum number of messages buffered per entity.
    constexpr QoS& keep_last(int depth) {
        history_ = KeepLast;
        depth_ = depth;
        return *this;
    }

    /// Use `KEEP_ALL` history (bounded by transport).
    constexpr QoS& keep_all() {
        history_ = KeepAll;
        return *this;
    }

    /// Subscriber max-inter-arrival / publisher offered-rate.
    /// `0` = infinite (no deadline check).
    constexpr QoS& deadline_ms(std::uint32_t ms) {
        deadline_ms_ = ms;
        return *this;
    }

    /// Sample expiry. `0` = infinite.
    constexpr QoS& lifespan_ms(std::uint32_t ms) {
        lifespan_ms_ = ms;
        return *this;
    }

    /// Liveliness kind. Pair with `liveliness_lease_ms()` for
    /// `MANUAL_BY_TOPIC` / `MANUAL_BY_NODE`. AUTOMATIC is the default.
    constexpr QoS& liveliness(Liveliness kind) {
        liveliness_ = kind;
        return *this;
    }

    /// Liveliness lease duration. `0` = infinite.
    constexpr QoS& liveliness_lease_ms(std::uint32_t ms) {
        liveliness_lease_ms_ = ms;
        return *this;
    }

    /// Skip the ROS `/rt/` topic-name prefix. Off by default; enable
    /// when interoperating with non-ROS DDS endpoints.
    constexpr QoS& avoid_ros_namespace_conventions(bool on) {
        avoid_ros_namespace_conventions_ = on ? 1 : 0;
        return *this;
    }

    // -- Predefined profiles (match rclcpp named constructors) --

    /// Default profile: `RELIABLE` + `VOLATILE` + `KEEP_LAST(10)`.
    static constexpr QoS default_profile() { return QoS(); }

    /// Sensor-data profile: `BEST_EFFORT` + `VOLATILE` + `KEEP_LAST(5)`.
    static constexpr QoS sensor_data() { return QoS().best_effort().keep_last(5); }

    /// Services profile: `RELIABLE` + `VOLATILE` + `KEEP_LAST(10)`.
    static constexpr QoS services() { return QoS().reliable(); }

    // -- Accessors --

    /// Reliability as a raw int (0 = Reliable, 1 = BestEffort).
    constexpr int reliability_raw() const { return static_cast<int>(reliability_); }
    /// Durability as a raw int (0 = Volatile, 1 = TransientLocal).
    constexpr int durability_raw() const { return static_cast<int>(durability_); }
    /// History as a raw int (0 = KeepLast, 1 = KeepAll).
    constexpr int history_raw() const { return static_cast<int>(history_); }
    /// Liveliness kind as a raw int (0..3).
    constexpr int liveliness_raw() const { return static_cast<int>(liveliness_); }
    /// Configured queue depth (only meaningful for `KEEP_LAST`).
    constexpr int depth() const { return depth_; }
    /// Deadline in ms (`0` = infinite).
    constexpr std::uint32_t deadline_ms() const { return deadline_ms_; }
    /// Lifespan in ms (`0` = infinite).
    constexpr std::uint32_t lifespan_ms() const { return lifespan_ms_; }
    /// Liveliness lease in ms (`0` = infinite).
    constexpr std::uint32_t liveliness_lease_ms() const { return liveliness_lease_ms_; }
    /// Whether to skip the `/rt/` ROS topic-name prefix.
    constexpr bool avoid_ros_namespace_conventions() const {
        return avoid_ros_namespace_conventions_ != 0;
    }

  private:
    enum Reliability { Reliable = 0, BestEffort = 1 } reliability_;
    enum Durability { Volatile = 0, TransientLocal = 1 } durability_;
    enum History { KeepLast = 0, KeepAll = 1 } history_;
    Liveliness liveliness_;
    int depth_;
    std::uint32_t deadline_ms_;
    std::uint32_t lifespan_ms_;
    std::uint32_t liveliness_lease_ms_;
    std::uint8_t avoid_ros_namespace_conventions_;
};

} // namespace nros

#endif // NROS_CPP_QOS_HPP
