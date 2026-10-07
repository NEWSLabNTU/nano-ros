#pragma once

#include <nros/nros.hpp>
#include <rclcpp_lifecycle/lifecycle_node.hpp>

#include "std_msgs.hpp"

namespace cpp_lifecycle_talker_pkg {

/// ManagedTalker — a C++ managed node written against
/// `rclcpp_lifecycle::LifecycleNode` (phase-482 W4), NOT the entry
/// `[lifecycle] autostart` codegen.
///
/// It IS a node (`SHAPE rclcpp`): the generated entry constructs it from the
/// executor-bound handle. It overrides the rclcpp-shape `on_*` hooks with
/// upstream's signatures, creates its publisher in `on_configure` as upstream
/// nodes do, and then self-drives the four REP-2002 transitions from its
/// constructor: Configure, Activate, Deactivate and Cleanup (printing the state
/// each leaves it in), then Configure and Activate again to run.
/// The publisher is a managed entity, so it sends only while the node is
/// Active — the `Published:` lines are the proof.
class ManagedTalker : public ::rclcpp_lifecycle::LifecycleNode {
    ::rclcpp_lifecycle::LifecyclePublisher<std_msgs::msg::Int32>::SharedPtr pub_;
    ::nros::Timer timer_;
    int32_t counter_ = 0;
    // phase-417 W4.f — declared on the node, read back on every tick, and the
    // value `ros2 param get` must agree with.
    int64_t publish_period_ms_ = 0;

    void on_tick();

  public:
    explicit ManagedTalker(::nros::NodeHandle h);

    CallbackReturn on_configure(const ::rclcpp_lifecycle::State& previous) override;
    CallbackReturn on_activate(const ::rclcpp_lifecycle::State& previous) override;
    CallbackReturn on_deactivate(const ::rclcpp_lifecycle::State& previous) override;
    CallbackReturn on_cleanup(const ::rclcpp_lifecycle::State& previous) override;
};

} // namespace cpp_lifecycle_talker_pkg
