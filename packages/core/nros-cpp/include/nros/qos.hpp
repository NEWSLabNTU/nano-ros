// nros-cpp: QoS (Quality of Service) profiles
// Freestanding C++ — no STL required

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
        : reliability_(Reliable),
          durability_(Volatile),
          history_(KeepLast),
          depth_(10) {}

    // -- Chainable setters (match rclcpp fluent API) --

    constexpr QoS& reliable() {
        reliability_ = Reliable;
        return *this;
    }

    constexpr QoS& best_effort() {
        reliability_ = BestEffort;
        return *this;
    }

    constexpr QoS& transient_local() {
        durability_ = TransientLocal;
        return *this;
    }

    constexpr QoS& durability_volatile() {
        durability_ = Volatile;
        return *this;
    }

    constexpr QoS& keep_last(int depth) {
        history_ = KeepLast;
        depth_ = depth;
        return *this;
    }

    constexpr QoS& keep_all() {
        history_ = KeepAll;
        return *this;
    }

    // -- Predefined profiles (match rclcpp named constructors) --

    static constexpr QoS default_profile() { return QoS(); }

    static constexpr QoS sensor_data() {
        return QoS().best_effort().keep_last(5);
    }

    static constexpr QoS services() {
        return QoS().reliable();
    }

    // -- Accessors --

    constexpr int reliability_raw() const { return static_cast<int>(reliability_); }
    constexpr int durability_raw() const { return static_cast<int>(durability_); }
    constexpr int history_raw() const { return static_cast<int>(history_); }
    constexpr int depth() const { return depth_; }

private:
    enum Reliability { Reliable = 0, BestEffort = 1 } reliability_;
    enum Durability  { Volatile = 0, TransientLocal = 1 } durability_;
    enum History     { KeepLast = 0, KeepAll = 1 } history_;
    int depth_;
};

} // namespace nros

#endif // NROS_CPP_QOS_HPP
