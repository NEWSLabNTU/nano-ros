The `rclcpp-compat-smoke` and `topic-state-monitor-port` templates now run
unmodified on FreeRTOS (mps2-an385) and Zephyr (mps2/an385) as well as posix,
each with a `mps2-an385-freertos/` and a `zephyr/` build directory beside its
source. A ported node whose `main` loops on `rclcpp::spin_some` and
`std::this_thread::sleep_for` works on FreeRTOS: the board now provides the
`sleep`/`usleep` and `gettimeofday` calls the C++ library makes, so
`std::chrono::steady_clock` counts from boot instead of returning garbage. On
Zephyr, `ament_target_dependencies(<name> …)` after
`nano_ros_add_executable(<name> …)` now reaches the `app` target that holds the
sources, so a dependency such as `diagnostic_updater` is on their include path.
