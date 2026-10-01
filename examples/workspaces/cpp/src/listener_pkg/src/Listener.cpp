// Listener — typed component (RFC-0043). Typed member callback on the generated
// `std_msgs::msg::Int32` (issue #218).

#include "listener_pkg/Listener.hpp"

#include <cstdio>

namespace listener_pkg {

void Listener::on_msg(const ::std_msgs::msg::Int32& msg) {
    std::printf("Received: %d\n", static_cast<int>(msg.data));
    ++recv_;
}

::rclcpp::Result Listener::configure(::rclcpp::Node& node) {
    // `::setvbuf` (C global), not `std::setvbuf` — Zephyr's picolibc <cstdio> does not put
    // setvbuf in namespace std; the C global is available on every platform.
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    // The QoS this subscription registers with. demo_bringup's
    // `system.contract.yaml` DECLARES the same (`qos: { depth: 1 }`), and the
    // build holds the two to one statement: if this line and the contract row
    // ever disagree -- depth, reliability or durability -- the assertion below
    // fails to COMPILE, naming the topic and both values (issue 1564).
    constexpr ::nros::QoS kChatterQos = ::nros::QoS(1);
    NROS_ASSERT_DECLARED_QOS(::std_msgs::msg::Int32::TYPE_NAME, "/chatter", kChatterQos,
                             "\"/chatter\"");
    // Typed member binding (RFC-0044): keyexpr + deserialize come from the
    // generated `std_msgs::msg::Int32` (issue #218 — hand-decode retired).
    return ::nros::bind_subscription<::std_msgs::msg::Int32, Listener, &Listener::on_msg>(
        node, "/chatter", this, kChatterQos);
}

} // namespace listener_pkg
