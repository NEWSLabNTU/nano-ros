#pragma once

#include <cstdint>

#include <nros/component_node.hpp>

namespace subnode_pkg {

/// Minimal type tag for std_msgs/Int32 (raw-CDR path).
struct Int32Tag {
    static constexpr const char *TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char *TYPE_HASH = "";
};

/// Phase 273 W4 (RFC-0047) — sub-node: ONE ComponentNode with TWO callback groups.
///
/// This is the RFC-0047 core proof: a single node that splits its callbacks across
/// two scheduling tiers via callback-group binding. The two groups ("ctrl" and
/// "telem") are declared in code (rclcpp shape) and assigned to workspace tiers in
/// system.toml `group_tiers` — the package itself carries NO tier reference, making
/// it portable across workspaces with different tier names.
///
/// - "ctrl"  group: 10 ms timer → publishes /ctrl  → bound to the "high" tier
/// - "telem" group: 100 ms timer → publishes /telem → bound to the "low" tier
///
/// The entry emits bind_group_sched("sub_node", "ctrl", SC_HIGH) and
/// bind_group_sched("sub_node", "telem", SC_LOW) before construction, so both
/// timers land on their respective sched contexts at registration.
class SubNode : public ::nros::ComponentNode {
    ::nros::Publisher<Int32Tag> ctrl_pub_;
    ::nros::Publisher<Int32Tag> telem_pub_;
    int ctrl_count_ = 0;
    int telem_count_ = 0;

    void on_ctrl();
    void on_telem();

  public:
    explicit SubNode(::nros::NodeHandle h);
};

} // namespace subnode_pkg
