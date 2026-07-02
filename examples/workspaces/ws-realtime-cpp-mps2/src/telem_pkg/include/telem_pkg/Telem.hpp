#pragma once

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace telem_pkg {

/// ws-realtime-cpp-mps2 — low-tier telemetry node. Publishes a monotonic
/// counter on /telem every 100 ms. The configure-shape (RFC-0043) receives a
/// Node& to create publishers and timers. Bound to the low-priority FreeRTOS
/// task (priority 2) via FreertosBoard::run_tiers (RFC-0015 §5 embedded).
class Telem {
    ::nros::Publisher<std_msgs::msg::Int32> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick();

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace telem_pkg
