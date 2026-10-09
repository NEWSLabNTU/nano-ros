An unmodified ROS 2 C++ program, `main` included, now runs on FreeRTOS and
Zephyr. Build it with `nano_ros_add_executable(<name> <sources> ROS2_MAIN)`:
the program's `main` is reached through the board's startup, and the ported
sources get the C++ standard library they were written against.
`examples/templates/cpp-port-minimal-publisher/` shows the build glue for
FreeRTOS (`mps2-an385-freertos/`) and Zephyr (`zephyr/`) beside the shared,
unmodified source. Zephyr `native_sim` is not supported for a ported program,
because its C library cannot host libstdc++; use a Cortex-M board such as
`mps2/an385`.

Fixed: on Zephyr, a source file that called `rclcpp::init` and included
`<rclcpp/rclcpp.hpp>` but not `<nros/main.hpp>` ignored
`CONFIG_NROS_ZENOH_LOCATOR` and connected to the guest's own loopback.
`nano_ros_add_executable` also refused a Zephyr leaf whose `system.toml` says
`[image.zephyr] board = "zephyr"`.
