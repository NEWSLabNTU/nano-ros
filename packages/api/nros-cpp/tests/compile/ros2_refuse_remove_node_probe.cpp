// NEGATIVE probe — phase-515 P2, `rclcpp::Executor::remove_node`.
//
// A node's entities live in the arena of the executor that created it, and
// arena registration is one-way (RFC-0002), so a node cannot leave it. The
// name exists so a ported `executor.remove_node(node)` fails HERE with that
// reason, not on "no member named 'remove_node'". `just check cpp` requires
// this TU to FAIL with the refusal text; the positive half is
// `executor_node_members_runtime.cpp`.

#include <rclcpp/rclcpp.hpp>

int ros2_refuse_remove_node_probe();
int ros2_refuse_remove_node_probe() {
    rclcpp::Executor executor;
    rclcpp::Node node;
    executor.remove_node(node.shared_from_this());
    return 0;
}
