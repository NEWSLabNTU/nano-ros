// BulkListener — typed component (RFC-0043) receiving a fragmented sample.

#include "bulk_listener_pkg/BulkListener.hpp"

#include <cstdio>

namespace bulk_listener_pkg {

void BulkListener::on_msg(const ::std_msgs::msg::String& msg) {
    ++recv_;
    // The leading decimal digits, up to the `:` the publisher puts after the
    // number. Parsed by hand, not with `strtoul`: under the FreeRTOS
    // toolchain's `-ffreestanding` the C++ library declares no `strtoul`,
    // in `std` or globally, so the image failed to compile.
    unsigned long seq = 0;
    for (const char* p = msg.data.c_str(); *p >= '0' && *p <= '9'; ++p) {
        seq = seq * 10 + static_cast<unsigned long>(*p - '0');
    }
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
