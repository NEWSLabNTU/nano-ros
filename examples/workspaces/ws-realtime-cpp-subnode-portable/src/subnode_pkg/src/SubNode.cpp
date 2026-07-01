// SubNode.cpp — Phase 273 W4 portability copy (RFC-0047).
// IDENTICAL to ws-realtime-cpp-subnode's SubNode.cpp.
// The tier names used at runtime come from deploy_bringup/system.toml
// ("fast"/"bulk") — no change to this file proves portability.
#include "subnode_pkg/SubNode.hpp"

#include <cstdio>
#include <cstring>

namespace subnode_pkg {

static void encode_int32(uint8_t buf[8], int32_t val) {
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    std::memcpy(buf + 4, &val, 4);
}

void SubNode::on_ctrl() {
    uint8_t buf[8];
    encode_int32(buf, static_cast<int32_t>(ctrl_count_));
    if (ctrl_pub_.publish_raw(buf, sizeof(buf)).ok()) {
        std::printf("[subnode/ctrl] tick=%d\n", ctrl_count_);
    }
    ctrl_count_++;
}

void SubNode::on_telem() {
    uint8_t buf[8];
    encode_int32(buf, static_cast<int32_t>(telem_count_));
    if (telem_pub_.publish_raw(buf, sizeof(buf)).ok()) {
        std::printf("[subnode/telem] tick=%d\n", telem_count_);
    }
    telem_count_++;
}

SubNode::SubNode(::nros::NodeHandle h)
    : ::nros::ComponentNode(h, "sub_node") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    auto ctrl_grp  = create_callback_group("ctrl");
    auto telem_grp = create_callback_group("telem");
    ctrl_pub_  = create_publisher<Int32Tag>("/ctrl");
    telem_pub_ = create_publisher<Int32Tag>("/telem");
    create_timer_in<SubNode, &SubNode::on_ctrl>(ctrl_grp, 10);
    create_timer_in<SubNode, &SubNode::on_telem>(telem_grp, 100);
}

} // namespace subnode_pkg
