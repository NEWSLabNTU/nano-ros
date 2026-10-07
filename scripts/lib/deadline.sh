# shellcheck shell=bash
# The ONE shell spelling of a deadline on a process (issues 1723, 1741).
#
#   . scripts/lib/deadline.sh
#   "${NROS_DEADLINE[@]}" 30 ros2 service call /add_two_ints ...
#   "${NROS_DEADLINE[@]}" 15 ./build/zephyr/zephyr.exe
#
# An ARRAY, not a function, so it composes with `exec`, `setsid`, `env` and a
# `run_bg` wrapper, which all need an executable rather than a shell function.
#
# Why not a bare `timeout N <cmd>`: `timeout` sends ONE SIGTERM and, without
# `--kill-after`, waits as long as its child does. A child that HANDLES SIGTERM
# is not bounded at all, and two kinds we run do:
#
#   * a ROS 2 CLI — rclpy installs a SIGTERM handler once `rclpy.init()`
#     returns, and over rmw_zenoh_cpp (Humble 0.1.9) a waiting CLI keeps
#     waiting. Measured 2026-10-07: one SIGTERM to a running `ros2 topic echo`
#     survived 3 of 3 on zenoh; four `timeout 20 ros2 topic echo` peers lived
#     4 days (issue 1723).
#   * a nano-ros image — the threadx-linux C examples catch SIGTERM, and a
#     `timeout 6 …/c_service_server` whose RTOS had wedged lived 34 days
#     (issue 1741). The image now ends itself within its own grace, which is
#     below this one; the deadline does not depend on that.
#
# Not `--foreground`, unlike the Rust spelling (`nros_tests::process::
# deadline`): a test harness owns its process group and reaps it, so its
# `timeout` must stay inside it. A shell script has no such owner, so here
# `timeout`'s OWN group kill is what takes `ros2 run`'s node down with the
# launcher. The grace is the same number on both sides, and
# `check-process-deadline` holds them equal.
NROS_KILL_GRACE_S=3
NROS_DEADLINE=(timeout "--kill-after=${NROS_KILL_GRACE_S}s")
