// ManagedTalker — a managed C++ node built on `rclcpp_lifecycle::LifecycleNode`
// (phase-482 W4; phase 270 #103 before it, on the retired `nros::LifecycleNode`
// mixin). Unlike LifecycleTalker (whose lifecycle is driven by the entry's
// `nros_cpp_lifecycle_autostart` codegen), this node authors its own transition
// behaviour through the rclcpp-shape on_* overrides and self-drives the machine
// from its constructor. `cpp_lifecycle_node_wrapper_e2e` greps this node's
// stdout for the transition markers and the gated `Published:` lines.

#include "cpp_lifecycle_talker_pkg/ManagedTalker.hpp"

#include <cstdio>

namespace cpp_lifecycle_talker_pkg {

ManagedTalker::CallbackReturn
ManagedTalker::on_configure(const ::rclcpp_lifecycle::State& /*previous*/) {
    pub_ = create_publisher<std_msgs::msg::Int32>("/chatter");
    std::printf("LC:on_configure\n");
    return pub_.get() != nullptr ? CallbackReturn::SUCCESS : CallbackReturn::FAILURE;
}

ManagedTalker::CallbackReturn
ManagedTalker::on_activate(const ::rclcpp_lifecycle::State& /*previous*/) {
    std::printf("LC:on_activate\n");
    return CallbackReturn::SUCCESS;
}

ManagedTalker::CallbackReturn
ManagedTalker::on_deactivate(const ::rclcpp_lifecycle::State& /*previous*/) {
    std::printf("LC:on_deactivate\n");
    return CallbackReturn::SUCCESS;
}

ManagedTalker::CallbackReturn
ManagedTalker::on_cleanup(const ::rclcpp_lifecycle::State& /*previous*/) {
    pub_.reset();
    std::printf("LC:on_cleanup\n");
    return CallbackReturn::SUCCESS;
}

void ManagedTalker::on_tick() {
    // Gated by the node's state: the publisher is a managed entity, and it is
    // activated only by a successful Activate (which proves on_activate ran).
    if (pub_.get() == nullptr || !pub_->is_activated()) {
        return;
    }
    std_msgs::msg::Int32 m;
    m.data = counter_++;
    if (pub_->publish(m).ok()) {
        std::printf("Published: %d\n", m.data);
    }
}

ManagedTalker::ManagedTalker(::nros::NodeHandle h)
    : ::rclcpp_lifecycle::LifecycleNode(h, "managed_talker") {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    // Declared on the node itself: it reaches the executor's one
    // `nros_params::ParameterServer`, the store the `rcl_interfaces` servers
    // read, so `ros2 param get /managed_talker publish_period_ms` answers with
    // this number.
    publish_period_ms_ = declare_parameter<int64_t>("publish_period_ms", 200);
    std::printf("LC:param publish_period_ms=%lld\n", static_cast<long long>(publish_period_ms_));

    if (!create_wall_timer<ManagedTalker, &ManagedTalker::on_tick>(
             timer_, static_cast<uint32_t>(publish_period_ms_), this)
             .ok()) {
        return;
    }
    // The services are registered by the base constructor. Run each of the four
    // transitions once and print the state it leaves the node in (2 = Inactive,
    // 3 = Active, 1 = Unconfigured) — phase-482 W4's runtime acceptance. The
    // second Configure re-creates the publisher, so the managed-entity link
    // also survives a move-assign into a member that already held one.
    const int s1 = configure().id();
    const int s2 = activate().id();
    const int s3 = deactivate().id();
    const int s4 = cleanup().id();
    std::printf("LC:cycle states=%d,%d,%d,%d\n", s1, s2, s3, s4);
    (void)configure();
    (void)activate();
    std::printf("LC:state=%d\n", static_cast<int>(get_current_state().id()));
}

} // namespace cpp_lifecycle_talker_pkg
