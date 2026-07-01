#pragma once

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace telem_pkg {

/// Minimal type tag for std_msgs/Int32 (raw-CDR path, RFC-0043).
struct Int32Tag {
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "";
};

/// Phase 274.W3 (ws-realtime-cpp-mps2) — low-tier telemetry node. Publishes a
/// monotonic counter on /telem every 100 ms. The configure-shape (RFC-0043) receives
/// a Node& to create publishers and timers. Bound to the low-priority FreeRTOS task
/// (priority 2) via FreertosBoard::run_tiers (RFC-0015 §5 embedded).
class Telem {
    ::nros::Publisher<Int32Tag> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick();

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace telem_pkg
