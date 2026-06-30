#pragma once

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace ctrl_pkg {

/// Minimal type tag for std_msgs/Int32 (raw-CDR path, RFC-0043).
/// Only TYPE_NAME is needed for publisher registration; the payload is
/// hand-encoded via publish_raw — no generated message header is consumed.
struct Int32Tag {
    static constexpr const char *TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char *TYPE_HASH = "";
};

/// Phase 269 W4 — high-tier control node. Publishes a monotonic counter on
/// /ctrl every 10 ms. The configure-shape (RFC-0043) receives a Node& to
/// create publishers and timers; the entry binds it to the high-priority sched
/// context via nros_cpp_node_create_ex (emitted by the W4 C++ codegen path).
class Ctrl {
    ::nros::Publisher<Int32Tag> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick();

  public:
    ::nros::Result configure(::nros::Node &node);
};

} // namespace ctrl_pkg
