# shellcheck shell=bash
# The ONE shell spelling of a deadline on a ROS 2 process (issue 1723).
#
#   . scripts/lib/ros2-deadline.sh
#   "${NROS_ROS2_DEADLINE[@]}" 30 ros2 service call /add_two_ints ...
#
# An ARRAY, not a function, so it composes with `exec`, `setsid`, `env` and a
# `run_bg` wrapper, which all need an executable rather than a shell function.
#
# Why not a bare `timeout N ros2 ...`: rclpy installs a SIGTERM handler once
# `rclpy.init()` returns, and that handler does not end the process — it
# triggers rclpy's guard conditions and returns. Over rmw_zenoh_cpp (Humble
# 0.1.9) a waiting CLI then keeps waiting. Measured 2026-10-07: one SIGTERM to
# a running `ros2 topic echo` survived 3 of 3 on zenoh and ended 3 of 3 on
# Cyclone and Fast-DDS. `timeout` without `--kill-after` never escalates, so it
# waits as long as the CLI does — a `timeout 25 ros2 service call` was alive
# 20 minutes later, and four `timeout 20 ros2 topic echo` peers lived 4 days.
#
# Not `--foreground`, unlike the Rust spelling (`nros_tests::ros2::
# ros2_deadline`): a test harness owns its process group and reaps it, so its
# `timeout` must stay inside it. A shell script has no such owner, so here
# `timeout`'s OWN group kill is what takes `ros2 run`'s node down with the
# launcher. The grace is the same number on both sides, and
# `check-ros2-cli-deadline` holds them equal.
NROS_ROS2_KILL_GRACE_S=3
NROS_ROS2_DEADLINE=(timeout "--kill-after=${NROS_ROS2_KILL_GRACE_S}s")
