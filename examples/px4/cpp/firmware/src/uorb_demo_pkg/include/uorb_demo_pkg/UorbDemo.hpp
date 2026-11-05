// nano-ros uORB interop demo -- the zero-serialization path (phase-325 W2),
// as a component the generated PX4 entry runs (phase-521 W8, RFC-0113 D9).
//
// On uORB a payload IS the PX4 struct: `publisher_create` resolves the topic to
// an `orb_metadata *` (registered below) and `publish_serialized` hands the
// bytes straight to `orb_publish`. So a STOCK PX4 module reads what this
// publishes, and this reads what PX4's commander publishes:
//
//     nros_uorb_demo start
//     listener debug_key_value
//
// `listener` is PX4's own command; a nano-ros-to-nano-ros check would pass a
// broken layout identically (issue 0351's thesis).
//
// No PX4 work item here: the module, its work item and its work queue
// are GENERATED from `uorb_demo_bringup` (one work item per sched task). A
// `vehicle_status` arrival schedules that work item from the publisher's
// context and dispatches `on_status`; the 1 s timer is its only deadline.
#pragma once

#include <rclcpp/rclcpp.hpp>

#include <stddef.h>
#include <stdint.h>

namespace uorb_demo_pkg {

class UorbDemo {
  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);

    uint32_t published() const { return published_; }
    uint32_t received() const { return received_; }

    /// One `vehicle_status` sample arrived; returns how many so far.
    uint32_t count_status() { return ++received_; }

  private:
    void on_tick();

    struct DebugKeyValueTag {
        static constexpr const char* TYPE_NAME = "px4_msgs::msg::DebugKeyValue";
        static constexpr const char* TYPE_HASH = "";
    };

    ::rclcpp::Publisher<DebugKeyValueTag> pub_{};
    ::rclcpp::Timer timer_{};
    uint32_t published_ = 0;
    uint32_t received_ = 0;
    float counter_ = 0.0F;

    // The `stats:` line's window (phase-521 W8 (e)): how far a tick strays
    // from 1 s, in PX4 time (virtual under SITL lockstep).
    uint64_t last_tick_us_ = 0;
    uint64_t jitter_max_us_ = 0;
};

} // namespace uorb_demo_pkg
