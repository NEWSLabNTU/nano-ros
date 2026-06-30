#pragma once

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace telem_pkg {

/// Minimal type tag for std_msgs/Int32 (raw-CDR path, RFC-0043).
struct Int32Tag {
    static constexpr const char *TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char *TYPE_HASH = "";
};

/// Phase 269 W4 — low-tier telemetry node. Publishes a monotonic counter on
/// /telem every 100 ms. Bound to the low-priority sched context by the W4
/// C++ codegen path (NodeBuilder::sched). Runs at 1/10 the cadence of ctrl_pkg;
/// the e2e test asserts ctrl publishes ≥3× as many messages as telem.
class Telem {
    ::nros::Publisher<Int32Tag> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick();

  public:
    ::nros::Result configure(::nros::Node &node);
};

} // namespace telem_pkg
