#pragma once

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace aux_pkg {

/// realtime-cpp — mid-tier auxiliary node. Publishes a monotonic counter on
/// /aux every 50 ms. The configure-shape (RFC-0043) receives a Node& to create
/// publishers and timers. Runs on the `mid` tier, which is all this node knows
/// about its own scheduling: the bringup assigns it
/// (`demo_bringup/system.toml`, `group_tiers = { aux = "mid" }`) and each
/// `[tiers.mid.*]` block there says what `mid` costs on one platform. That tier
/// is spawned BY a spawned tier (boot→mid→low), so it is the middle hop the
/// #144 chained-spawn fix serializes (RFC-0015 Model 1).
class Aux {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace aux_pkg
