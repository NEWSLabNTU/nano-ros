#pragma once

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace ctrl_pkg {

/// ws-realtime-cpp-mps2 — high-tier control node. Publishes a monotonic
/// counter on /ctrl every 10 ms. The configure-shape (RFC-0043) receives a
/// Node& to create publishers and timers. The entry binds it to the
/// high-priority freertos task (priority 5) via FreertosBoard::run_tiers
/// (RFC-0015 §5 embedded).
class Ctrl {
    ::nros::Publisher<std_msgs::msg::Int32> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick();

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace ctrl_pkg
