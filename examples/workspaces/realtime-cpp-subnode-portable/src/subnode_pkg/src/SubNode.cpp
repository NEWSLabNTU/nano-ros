// SubNode.cpp — portability copy (RFC-0047).
// IDENTICAL to ws-realtime-cpp-subnode's SubNode.cpp.
// The tier names used at runtime come from deploy_bringup/system.toml
// ("fast"/"bulk") — no change to this file proves portability.
#include "subnode_pkg/SubNode.hpp"

#include <cstdio>

namespace subnode_pkg {

void SubNode::on_ctrl() {
    std_msgs::msg::Int32 msg;
    msg.data = ctrl_count_;
    if (ctrl_pub_.publish(msg).ok()) {
        std::printf("[subnode/ctrl] tick=%d\n", ctrl_count_);
    }
    ctrl_count_++;
}

void SubNode::on_telem() {
    std_msgs::msg::Int32 msg;
    msg.data = telem_count_;
    if (telem_pub_.publish(msg).ok()) {
        std::printf("[subnode/telem] tick=%d\n", telem_count_);
    }
    telem_count_++;
}

SubNode::SubNode(::nros::NodeHandle h) : ::nros::ComponentNode(h, "sub_node") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    auto ctrl_grp = create_callback_group("ctrl");
    auto telem_grp = create_callback_group("telem");
    ctrl_pub_ = create_publisher<std_msgs::msg::Int32>("/ctrl");
    telem_pub_ = create_publisher<std_msgs::msg::Int32>("/telem");
    create_timer_in<SubNode, &SubNode::on_ctrl>(ctrl_grp, 10);
    create_timer_in<SubNode, &SubNode::on_telem>(telem_grp, 100);
}

} // namespace subnode_pkg
