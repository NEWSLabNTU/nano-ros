// NEGATIVE probe — phase-488, `rclcpp::executors::MultiThreadedExecutor`.
//
// The name exists so a ported `main` fails HERE, with RFC-0002's reason and the
// alternative (`SingleThreadedExecutor`, or a tier per parallel workload),
// rather than on "'executors' has not been declared". Declaring one is the
// error. `just check cpp` requires this TU to FAIL with the refusal text; the
// positive half is `rclcpp-examples/ported/multithreaded_executor.cpp`, which
// spins `SingleThreadedExecutor`.

#include <rclcpp/rclcpp.hpp>

int ros2_refuse_multi_threaded_executor_probe();
int ros2_refuse_multi_threaded_executor_probe() {
    rclcpp::executors::MultiThreadedExecutor executor;
    (void)executor;
    return 0;
}
