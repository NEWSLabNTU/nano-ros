// BulkListener — typed component (RFC-0043) receiving a fragmented sample.

#include "bulk_listener_pkg/BulkListener.hpp"

#include <cstdio>
#include <cstdlib>

namespace bulk_listener_pkg {

void BulkListener::on_msg(const ::std_msgs::msg::String& msg) {
    ++recv_;
    // `strtoul` stops at the `:` the publisher puts after the number.
    unsigned long seq = std::strtoul(msg.data.c_str(), nullptr, 10);
    std::printf("Received bulk: seq=%lu len=%u n=%lu\n", seq,
                static_cast<unsigned>(msg.data.length()), recv_);
}

::rclcpp::Result BulkListener::configure(::rclcpp::Node& node) {
    // `::setvbuf` (C global): Zephyr's picolibc <cstdio> has no std::setvbuf.
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    // Reliable, KEEP_LAST(1). The reader is reliable so a lost fragment is
    // repaired (NACKFRAG) rather than costing one sample — which is exactly
    // the path issue 0917 found stalling; depth 1 is enough for a 10 Hz
    // publisher whose every sample is printed on arrival.
    constexpr ::rclcpp::QoS kBulkQos = ::rclcpp::QoS(1);
    return ::rclcpp::bind_subscription<::std_msgs::msg::String, BulkListener,
                                       &BulkListener::on_msg>(node, "/bulk", this, kBulkQos);
}

} // namespace bulk_listener_pkg
