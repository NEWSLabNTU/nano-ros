#include "uorb_demo_pkg/UorbDemo.hpp"

#include <rclcpp/rclcpp.hpp>

#include "nros_rmw_uorb.h"
#include "nros_rmw_uorb_registry.h"

// PX4 after nano-ros: PX4's `defines.h` defines `ERROR`.
#include <drivers/drv_hrt.h>
#include <px4_platform_common/log.h>
#include <uORB/topics/debug_key_value.h>
#include <uORB/topics/vehicle_status.h>

#include <cstring>

namespace uorb_demo_pkg {

namespace {

constexpr const char* kDebugTopic = "/fmu/out/debug_key_value";
constexpr const char* kStatusTopic = "/fmu/out/vehicle_status";

// The PAYLOAD type is PX4's own struct. There is deliberately no generated
// binding: a generated one would describe a CDR layout, and CDR is exactly
// what does not happen on uORB. Deserializing is a copy of the struct.
struct VehicleStatus {
    static constexpr const char* TYPE_NAME = "px4_msgs::msg::VehicleStatus";
    static constexpr const char* TYPE_HASH = "";
    static constexpr size_t SERIALIZED_SIZE_MAX = sizeof(vehicle_status_s);

    vehicle_status_s s;

    static int ffi_deserialize(const uint8_t* data, size_t len, VehicleStatus* out) {
        if (data == nullptr || out == nullptr || len < sizeof(vehicle_status_s)) {
            return -1;
        }
        std::memcpy(&out->s, data, sizeof(vehicle_status_s));
        return 0;
    }
};

::rclcpp::Subscription<VehicleStatus> g_status_sub{};
UorbDemo* g_self = nullptr;

void on_status(const VehicleStatus& msg) {
    if (g_self == nullptr) {
        return;
    }
    const uint32_t n = g_self->count_status();
    if (n == 1 || (n % 10) == 0) {
        PX4_INFO("recv vehicle_status: nav_state=%u arming_state=%u (%u samples)",
                 static_cast<unsigned>(msg.s.nav_state), static_cast<unsigned>(msg.s.arming_state),
                 static_cast<unsigned>(n));
    }
}

} // namespace

::rclcpp::Result UorbDemo::configure(::rclcpp::Node& node) {
    // uORB has no name-keyed metadata lookup of its own: teach the backend
    // which `orb_metadata` each ROS-style name means.
    if (nros_rmw_uorb_register_topic(kDebugTopic, DebugKeyValueTag::TYPE_NAME,
                                     ORB_ID(debug_key_value)) != NROS_RMW_RET_OK ||
        nros_rmw_uorb_register_topic(kStatusTopic, VehicleStatus::TYPE_NAME,
                                     ORB_ID(vehicle_status)) != NROS_RMW_RET_OK) {
        PX4_ERR("register_topic failed");
        return ::rclcpp::Result(::rclcpp::ErrorCode::Error);
    }
    g_self = this;
    ::rclcpp::Result r = node.create_publisher(pub_, kDebugTopic, ::rclcpp::QoS(1));
    if (!r.ok()) {
        PX4_ERR("create_publisher(%s) failed (%d)", kDebugTopic, static_cast<int>(r.raw()));
        return r;
    }
    r = node.create_subscription(g_status_sub, kStatusTopic, ::rclcpp::QoS(1), &on_status);
    if (!r.ok()) {
        PX4_ERR("create_subscription(%s) failed (%d)", kStatusTopic, static_cast<int>(r.raw()));
        return r;
    }
    r = node.create_wall_timer<UorbDemo, &UorbDemo::on_tick>(timer_, 1000, this);
    if (!r.ok()) {
        PX4_ERR("create_wall_timer failed (%d)", static_cast<int>(r.raw()));
    }
    return r;
}

void UorbDemo::on_tick() {
    // No encode step, and none skipped: `msg` IS what lands in the uORB queue.
    debug_key_value_s msg{};
    msg.timestamp = hrt_absolute_time();
    std::strncpy(msg.key, "nros", sizeof(msg.key) - 1);
    msg.value = counter_;
    if (pub_.publish_serialized(reinterpret_cast<const uint8_t*>(&msg), sizeof(msg)).ok()) {
        published_++;
        counter_ += 1.0F;
    } else {
        PX4_ERR("publish_serialized failed");
    }
    if (published_ == 1 || (published_ % 10) == 0) {
        PX4_INFO("published debug_key_value key=nros value=%.1f (%u samples)",
                 static_cast<double>(msg.value), static_cast<unsigned>(published_));
    }
}

} // namespace uorb_demo_pkg
