#include "uorb_bridge_pkg/UorbBridge.hpp"

#include <rclcpp/rclcpp.hpp>

#include "nros_rmw_uorb.h"
#include "nros_rmw_uorb_registry.h"

// PX4 after nano-ros: PX4's `defines.h` defines `ERROR`.
#include <px4_platform_common/log.h>
#include <uORB/topics/debug_key_value.h>

#include <cstdlib>
#include <cstring>

// The outward backend, chosen when libnros_cpp.a was built
// (`--features rmw-<name>-cffi`) and named here so session selection does not
// depend on registration order.
#ifndef NROS_BRIDGE_RMW
#define NROS_BRIDGE_RMW "zenoh"
#endif

namespace uorb_bridge_pkg {

namespace {

// The uORB side: a ROS-style name mapped to `ORB_ID(debug_key_value)` below.
constexpr const char* kInTopic = "/fmu/out/debug_key_value";
// The wire side: the topic a ROS 2 node subscribes.
constexpr const char* kOutTopic = "/fmu/out/debug_key_value";

// Where the outward session dials. `$NROS_LOCATOR` overrides; this is the zenoh
// default endpoint so a plain router on the same host just works.
constexpr const char* kDefaultOutLocator = "tcp/127.0.0.1:7447";

// The INWARD payload is PX4's own struct, verbatim: deserializing is a copy.
struct DebugKeyValueIn {
    static constexpr const char* TYPE_NAME = px4_msgs::msg::DebugKeyValue::TYPE_NAME;
    static constexpr const char* TYPE_HASH = "";
    static constexpr size_t SERIALIZED_SIZE_MAX = sizeof(debug_key_value_s);

    debug_key_value_s s;

    static int ffi_deserialize(const uint8_t* data, size_t len, DebugKeyValueIn* out) {
        if (data == nullptr || out == nullptr || len < sizeof(debug_key_value_s)) {
            return -1;
        }
        std::memcpy(&out->s, data, sizeof(debug_key_value_s));
        return 0;
    }
};

/// The whole point of the module: PX4 struct → ROS/CDR struct.
///
/// Field-by-field because the layouts differ (see the header comment). Kept as a
/// free function so it is trivially unit-testable and so a second topic is a
/// second function rather than more branches in `Run()`.
void translate(const debug_key_value_s& in, px4_msgs::msg::DebugKeyValue& out) {
    out.timestamp = in.timestamp;
    out.value = in.value;
    // `char[10]`, NUL-terminated. Copy at most capacity-1 and terminate, so the
    // CDR side's `fixed_str` (which stops at the first NUL) never reads past the
    // data — the bytes after a terminator are uninitialized.
    static_assert(sizeof(out.key) >= sizeof(in.key), "ROS key buffer smaller than PX4's");
    std::memset(out.key, 0, sizeof(out.key));
    std::memcpy(out.key, in.key, sizeof(in.key));
    out.key[sizeof(out.key) - 1] = '\0';
}


::rclcpp::Subscription<DebugKeyValueIn> g_in_sub{};
UorbBridge* g_self = nullptr;

void on_sample(const DebugKeyValueIn& in) {
    if (g_self == nullptr) {
        return;
    }
    px4_msgs::msg::DebugKeyValue out{};
    translate(in.s, out);
    g_self->forward(out);
}

} // namespace

::rclcpp::Result UorbBridge::configure(::rclcpp::Node& node) {
    // uORB has no name-keyed metadata lookup, so teach the backend which
    // orb_metadata the inward ROS-style name means.
    if (nros_rmw_uorb_register_topic(kInTopic, px4_msgs::msg::DebugKeyValue::TYPE_NAME,
                                     ORB_ID(debug_key_value)) != NROS_RMW_RET_OK) {
        PX4_ERR("register_topic(%s) failed", kInTopic);
        return ::rclcpp::Result(::rclcpp::ErrorCode::Error);
    }
    g_self = this;

    // The outward node, on the named networked session.
    const char* out_locator = std::getenv("NROS_LOCATOR");
    if (out_locator == nullptr || out_locator[0] == '\0') {
        out_locator = kDefaultOutLocator;
    }
    PX4_INFO("outward %s locator: %s", NROS_BRIDGE_RMW, out_locator);
    ::rclcpp::Result r = ::rclcpp::NodeBuilder(node.executor_handle(), "px4_bridge_out")
                             .rmw(NROS_BRIDGE_RMW)
                             .locator(out_locator)
                             .build(out_node_);
    if (!r.ok()) {
        PX4_ERR("bind outward node to %s failed (%d)", NROS_BRIDGE_RMW, static_cast<int>(r.raw()));
        return r;
    }
    r = out_node_.create_publisher(out_pub_, kOutTopic, ::rclcpp::QoS(10));
    if (!r.ok()) {
        PX4_ERR("create_publisher(%s) on %s failed (%d)", kOutTopic, NROS_BRIDGE_RMW,
                static_cast<int>(r.raw()));
        return r;
    }
    r = node.create_subscription(g_in_sub, kInTopic, ::rclcpp::QoS(1), &on_sample);
    if (!r.ok()) {
        PX4_ERR("create_subscription(%s) failed (%d)", kInTopic, static_cast<int>(r.raw()));
        return r;
    }
    PX4_INFO("bridging %s (uorb) -> %s (%s)", kInTopic, kOutTopic, NROS_BRIDGE_RMW);
    return r;
}

void UorbBridge::forward(const px4_msgs::msg::DebugKeyValue& out) {
    if (out_pub_.publish(out).ok()) {
        forwarded_++;
        if (forwarded_ == 1 || (forwarded_ % 25) == 0) {
            PX4_INFO("forwarded %u samples (key=%s value=%.1f)", static_cast<unsigned>(forwarded_),
                     out.key, static_cast<double>(out.value));
        }
    } else {
        drops_++;
    }
}

} // namespace uorb_bridge_pkg
