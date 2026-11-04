// Listener — the ROS 2 tutorial listener as a typed component (RFC-0043). Typed member
// callback on the generated `std_msgs::msg::String` (issue #218).

#include "listener_pkg/Listener.hpp"

#include <cstdio>

namespace listener_pkg {

void Listener::on_msg(const ::std_msgs::msg::String& msg) {
    ++recv_;
    std::printf("I heard: [%s]\n", msg.data.c_str());
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
    constexpr ::rclcpp::QoS kChatterQos = ::rclcpp::QoS(1);
    NROS_ASSERT_DECLARED_QOS(::std_msgs::msg::String::TYPE_NAME, "/chatter", kChatterQos,
                             "\"/chatter\"");
    // Typed member binding (RFC-0044): keyexpr + deserialize come from the
    // generated `std_msgs::msg::String` (issue #218 — hand-decode retired).
    ::rclcpp::Result r =
        ::rclcpp::bind_subscription<::std_msgs::msg::String, Listener, &Listener::on_msg>(
            node, "/chatter", this, kChatterQos);
    if (r.ok()) {
        // The readiness marker the test matrix waits on
        // (`nros_tests::output::LISTENER_READY_MARKER`), as every tutorial listener prints it.
        std::printf("Subscriber created for topic: %s\n", "/chatter");
    }
    return r;
}

} // namespace listener_pkg
