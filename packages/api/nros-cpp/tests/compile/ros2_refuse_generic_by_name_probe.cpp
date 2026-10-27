/*
 * NEGATIVE probe — phase-515 P4.
 *
 * rclcpp's `create_generic_publisher(topic, type, qos)` names a type and loads
 * its typesupport at runtime; nano-ros needs the type's RIHS01 hash on the
 * wire and cannot derive it from a name. The name-only spelling EXISTS and
 * fails with `NROS_RCLCPP_REFUSE_GENERIC_BY_NAME`, which names the
 * hash-taking form; absence would say only "no member named ...".
 *
 * `just check cpp` requires this TU to FAIL with that message. The hash-taking
 * form is exercised by `generic_pubsub_runtime.cpp`.
 */

#include <rclcpp/rclcpp.hpp>

void ported(rclcpp::Node& node) {
    node.create_generic_publisher("chatter", "std_msgs/msg/String", 10);
}
