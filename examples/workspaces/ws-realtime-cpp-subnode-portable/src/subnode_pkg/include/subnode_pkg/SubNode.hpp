#pragma once

#include <cstdint>

#include <nros/component_node.hpp>

namespace subnode_pkg {

/// Minimal type tag for std_msgs/Int32 (raw-CDR path).
struct Int32Tag {
    static constexpr const char *TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char *TYPE_HASH = "";
};

/// Phase 273 W4 (RFC-0047) — portability copy of SubNode. IDENTICAL to
/// ws-realtime-cpp-subnode's SubNode. Deployed here with "fast"/"bulk" tier names
/// instead of "high"/"low" — no package change, tier binding from system.toml.
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
