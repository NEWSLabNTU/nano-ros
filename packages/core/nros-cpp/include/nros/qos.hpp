// nros-cpp: QoS (Quality of Service) profiles
// Freestanding C++ — no STL required

/**
 * @file qos.hpp
 * @ingroup grp_qos
 * @brief `nros::QoS` — reliability/durability/history settings.
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
class QoS {
  public:
    /// Default QoS: reliable, volatile, keep-last(10).
    constexpr QoS()
        : reliability_(Reliable), durability_(Volatile), history_(KeepLast), depth_(10) {}

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

    // -- Predefined profiles (match rclcpp named constructors) --

    /// Default profile: `RELIABLE` + `VOLATILE` + `KEEP_LAST(10)`.
    static constexpr QoS default_profile() { return QoS(); }

    /// Sensor-data profile: `BEST_EFFORT` + `VOLATILE` + `KEEP_LAST(5)`.
    static constexpr QoS sensor_data() { return QoS().best_effort().keep_last(5); }

    /// Services profile: `RELIABLE`.
    static constexpr QoS services() { return QoS().reliable(); }

    // -- Accessors --

    /// Reliability as a raw int (0 = Reliable, 1 = BestEffort).
    constexpr int reliability_raw() const { return static_cast<int>(reliability_); }
    /// Durability as a raw int (0 = Volatile, 1 = TransientLocal).
    constexpr int durability_raw() const { return static_cast<int>(durability_); }
    /// History as a raw int (0 = KeepLast, 1 = KeepAll).
    constexpr int history_raw() const { return static_cast<int>(history_); }
    /// Configured queue depth (only meaningful for `KEEP_LAST`).
    constexpr int depth() const { return depth_; }

  private:
    enum Reliability { Reliable = 0, BestEffort = 1 } reliability_;
    enum Durability { Volatile = 0, TransientLocal = 1 } durability_;
    enum History { KeepLast = 0, KeepAll = 1 } history_;
    int depth_;
};

} // namespace nros

#endif // NROS_CPP_QOS_HPP
